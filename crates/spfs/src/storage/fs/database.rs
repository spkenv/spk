// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use encoding::prelude::*;
use futures::{Stream, StreamExt, TryFutureExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::graph::{DatabaseView, Object, ObjectProto};
use crate::{Error, Result, encoding, graph};

#[cfg(test)]
#[path = "./database_test.rs"]
mod database_test;

/// Inspect a path without hiding metadata errors or following its final symlink.
async fn describe_path(path: &Path) -> String {
    let status = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_file() => format!("file ({} bytes)", metadata.len()),
        Ok(metadata) if metadata.is_dir() => "directory".to_owned(),
        Ok(metadata) if metadata.is_symlink() => "symlink".to_owned(),
        Ok(_) => "other file type".to_owned(),
        Err(err) => format!("metadata error: {err}"),
    };
    format!("{}: {status}", path.display())
}

/// Capture rename diagnostics before cleanup removes evidence of the failure.
///
/// These observations can change after the failed operation, especially on NFS,
/// and must not be used to infer success or suppress the original error.
async fn object_rename_error(
    source_path: &Path,
    destination_path: &Path,
    source: std::io::Error,
) -> Error {
    let mut observations = vec![
        format!("source: {}", describe_path(source_path).await),
        format!("destination: {}", describe_path(destination_path).await),
    ];
    for (label, path) in [
        ("source parent", source_path.parent()),
        ("destination parent", destination_path.parent()),
    ] {
        if let Some(path) = path {
            observations.push(format!("{label}: {}", describe_path(path).await));
        }
    }
    Error::StorageObjectRenameError {
        source_path: source_path.to_owned(),
        destination_path: destination_path.to_owned(),
        observations: observations.join("\n"),
        source,
    }
}

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

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
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
    ) -> Result<encoding::Digest> {
        self.opened().await?.resolve_full_digest(partial).await
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

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        Box::pin(self.objects.find(search_criteria))
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
    ) -> Result<encoding::Digest> {
        self.objects.resolve_full_digest(partial).await
    }
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

        // this might fail but we don't consider that fatal just yet
        #[cfg(unix)]
        let _ = tokio::fs::set_permissions(&filepath, std::fs::Permissions::from_mode(0o777)).await;

        let metadata = tokio::fs::symlink_metadata(&filepath)
            .await
            .map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => Error::UnknownObject(digest),
                _ => Error::StorageReadError(
                    "symlink_metadata on digest path",
                    filepath.clone(),
                    err,
                ),
            })?;

        let mtime = metadata.modified().map_err(|err| {
            Error::StorageReadError(
                "modified on symlink metadata of digest path",
                filepath.clone(),
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
                    "remove_file on object file in remove_object_if_older_than",
                    filepath,
                    err,
                )),
            };
        }
        Ok(true)
    }
}

#[async_trait::async_trait]
impl graph::DatabaseExt for super::OpenFsRepository {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
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
        if let Err(err) = writer.into_inner().sync_all().await {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(
                "sync_all on object file",
                working_file.clone(),
                err,
            ));
        };
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
                let err = object_rename_error(&working_file, &filepath, err).await;
                let _ = tokio::fs::remove_file(&working_file).await;
                Err(err)
            }
        }
    }
}
