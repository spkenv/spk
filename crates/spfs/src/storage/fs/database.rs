// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use close_err::Closable;
use encoding::prelude::*;
use futures::{Stream, StreamExt, TryFutureExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::graph::{DatabaseView, FoundDigest, Object, ObjectProto};
use crate::{Error, Result, encoding, graph};

#[async_trait::async_trait]
impl DatabaseView for super::MaybeOpenFsRepository {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        let Ok(opened) = self.opened().await else {
            return false;
        };
        opened.has_object(digest).await
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        self.opened().await?.read_object(digest).await
    }

    fn find_digests<'a>(
        &self,
        search_criteria: &'a graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<FoundDigest>> + Send + 'a>> {
        self.opened()
            .map_ok(|opened| opened.find_digests(search_criteria))
            .try_flatten_stream()
            .boxed()
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        graph::DatabaseIterator::new(self)
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        graph::DatabaseWalker::new(self, *root)
    }

    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
        partial_digest_type: graph::PartialDigestType,
    ) -> Result<FoundDigest> {
        self.opened()
            .await?
            .resolve_full_digest(partial, partial_digest_type)
            .await
    }
}

#[async_trait::async_trait]
impl graph::Database for super::MaybeOpenFsRepository {
    async fn remove_object(&self, digest: encoding::Digest) -> crate::Result<()> {
        self.opened().await?.remove_object(digest).await
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> crate::Result<bool> {
        self.opened()
            .await?
            .remove_object_if_older_than(older_than, digest)
            .await
    }
}

#[async_trait::async_trait]
impl graph::DatabaseExt for super::MaybeOpenFsRepository {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        self.opened().await?.write_object(obj).await
    }

    async unsafe fn write_object_unchecked<T: ObjectProto>(
        &self,
        obj: &graph::FlatObject<T>,
    ) -> Result<()> {
        // Safety: transitive unsafe call
        unsafe { self.opened().await?.write_object_unchecked(obj).await }
    }
}

#[async_trait::async_trait]
impl DatabaseView for super::OpenFsRepository {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        let filepath = self.objects.build_digest_path(&digest);
        tokio::fs::symlink_metadata(filepath).await.is_ok()
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        let filepath = self.objects.build_digest_path(&digest);
        let mut file =
            tokio::io::BufReader::new(tokio::fs::File::open(&filepath).await.map_err(|err| {
                match err.kind() {
                    std::io::ErrorKind::NotFound => Error::UnknownObject(digest),
                    _ => Error::StorageReadError("open object file", filepath.clone(), err),
                }
            })?);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|err| {
            Error::StorageReadError("read_to_end on object file", filepath.clone(), err)
        })?;
        // if the capacity of the vec already equals the length then a conversion
        // to the Bytes type in the `new` call will avoid reallocating the data
        buf.shrink_to_fit();
        Object::new(buf)
    }

    fn find_digests<'a>(
        &self,
        search_criteria: &'a graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<FoundDigest>> + Send + 'a>> {
        Box::pin(
            self.objects
                .find(search_criteria)
                .map(|d| d.map(FoundDigest::Object))
                .chain(
                    self.payloads
                        .find(search_criteria)
                        .map(|d| d.map(FoundDigest::Payload)),
                ),
        )
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        graph::DatabaseIterator::new(self)
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        graph::DatabaseWalker::new(self, *root)
    }

    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
        partial_digest_type: graph::PartialDigestType,
    ) -> Result<FoundDigest> {
        match (
            partial_digest_type,
            self.objects
                .resolve_full_digest(partial)
                .map_ok(FoundDigest::Object),
            self.payloads
                .resolve_full_digest(partial)
                .map_ok(FoundDigest::Payload),
        ) {
            (graph::PartialDigestType::Object, f, _) => f.await,
            (graph::PartialDigestType::Payload, _, f) => f.await,
            (graph::PartialDigestType::Unknown, object_f, payload_f) => {
                object_f.or_else(|_| payload_f).await
            }
        }
    }
}

/// Remove the file at `filepath` if it is older than `older_than`.
///
/// Returns true if the file was removed, false if it was not.
pub(crate) async fn remove_file_if_older_than(
    older_than: DateTime<Utc>,
    filepath: &Path,
    digest: encoding::Digest,
) -> crate::Result<bool> {
    let metadata = tokio::fs::symlink_metadata(&filepath)
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => Error::UnknownObject(digest),
            _ => {
                Error::StorageReadError("symlink_metadata on digest path", filepath.to_owned(), err)
            }
        })?;

    let mtime = metadata.modified().map_err(|err| {
        Error::StorageReadError(
            "modified on symlink metadata of digest path",
            filepath.to_owned(),
            err,
        )
    })?;

    if DateTime::<Utc>::from(mtime) >= older_than {
        return Ok(false);
    }

    if let Err(err) = tokio::fs::remove_file(&filepath).await {
        return match err.kind() {
            std::io::ErrorKind::NotFound => Ok(true),
            _ => Err(Error::StorageWriteError(
                "remove_file in remove_file_if_older_than",
                filepath.to_owned(),
                err,
            )),
        };
    }
    Ok(true)
}

#[async_trait::async_trait]
impl graph::Database for super::OpenFsRepository {
    async fn remove_object(&self, digest: encoding::Digest) -> crate::Result<()> {
        let filepath = self.objects.build_digest_path(&digest);

        // this might fail but we don't consider that fatal just yet
        #[cfg(unix)]
        let _ = tokio::fs::set_permissions(&filepath, std::fs::Permissions::from_mode(0o777)).await;

        if let Err(err) = tokio::fs::remove_file(&filepath).await {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(Error::StorageWriteError(
                    "remove_file on object file in remove_object",
                    filepath,
                    err,
                )),
            };
        }
        tracing::trace!(%digest, "removed object from db");
        Ok(())
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> crate::Result<bool> {
        let filepath = self.objects.build_digest_path(&digest);
        remove_file_if_older_than(older_than, &filepath, digest).await
    }
}

#[async_trait::async_trait]
impl graph::DatabaseExt for super::OpenFsRepository {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        if matches!(obj.to_enum(), graph::object::Enum::Blob(_)) {
            return Err("writing blob objects is not permitted".into());
        };

        // Safety: we checked that the object is not a blob above
        unsafe { self.write_object_unchecked(obj).await }
    }

    async unsafe fn write_object_unchecked<T: ObjectProto>(
        &self,
        obj: &graph::FlatObject<T>,
    ) -> Result<()> {
        let digest = obj.digest()?;
        let filepath = self.objects.build_digest_path(&digest);
        if filepath.exists() {
            tracing::trace!(%digest, kind=%std::any::type_name::<T>(), "object already exists");
            return Ok(());
        }
        tracing::trace!(%digest, kind=%std::any::type_name::<T>(), "writing object to db");

        // we need to use a temporary file here, so that
        // other processes don't try to read our incomplete
        // object from the database
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_file = self.objects.workdir().join(uuid);
        self.objects.ensure_base_dir(&working_file)?;
        let mut encoded = Vec::new();
        obj.encode(&mut encoded)?;
        let mut writer = tokio::io::BufWriter::new(
            tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&working_file)
                .await
                .map_err(|err| {
                    Error::StorageWriteError(
                        "open on object file for write",
                        working_file.clone(),
                        err,
                    )
                })?,
        );
        if let Err(err) = writer.write_all(encoded.as_slice()).await {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(
                "write_all on object file",
                working_file,
                err,
            ));
        }
        if let Err(err) = writer.flush().await {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(
                "flush on object file",
                working_file,
                err,
            ));
        }
        if let Err(err) = writer.into_inner().into_std().await.close() {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(
                "close on object file",
                working_file.clone(),
                err,
            ));
        }
        #[cfg(unix)]
        {
            let perms = std::fs::Permissions::from_mode(self.objects.file_permissions);
            if let Err(err) = tokio::fs::set_permissions(&working_file, perms).await {
                let _ = tokio::fs::remove_file(&working_file).await;
                return Err(Error::StorageWriteError(
                    "set permissions on object file",
                    working_file,
                    err,
                ));
            }
        }
        self.objects.ensure_base_dir(&filepath)?;
        match tokio::fs::rename(&working_file, &filepath).await {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = tokio::fs::remove_file(&working_file).await;
                Err(Error::StorageWriteError(
                    "rename on object file",
                    filepath,
                    err,
                ))
            }
        }
    }
}
