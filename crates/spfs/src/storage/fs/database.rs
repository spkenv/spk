// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::PermissionsExt;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use close_err::Closable;
use encoding::{Decodable, Encodable};
use futures::Stream;
use graph::DatabaseView;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::graph::Object;
use crate::{encoding, graph, Error, Result};

#[async_trait::async_trait]
impl DatabaseView for super::FSRepository {
    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        let filepath = self.objects.build_digest_path(&digest);
        let mut file =
            tokio::io::BufReader::new(tokio::fs::File::open(&filepath).await.map_err(|err| {
                match err.kind() {
                    std::io::ErrorKind::NotFound => Error::UnknownObject(digest),
                    _ => Error::StorageReadError(filepath.clone(), err),
                }
            })?);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .map_err(|err| Error::StorageReadError(filepath.clone(), err))?;
        Object::decode(&mut buf.as_slice())
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
impl graph::Database for super::FSRepository {
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        let digest = obj.digest()?;
        let filepath = self.objects.build_digest_path(&digest);
        if filepath.exists() {
            tracing::trace!(?digest, "object already exists");
            return Ok(());
        }
        tracing::trace!(?digest, kind = ?obj.kind(), "writing object to db");

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
                .map_err(|err| Error::StorageWriteError(working_file.clone(), err))?,
        );
        if let Err(err) = writer.write_all(encoded.as_slice()).await {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(working_file, err));
        }
        if let Err(err) = writer.flush().await {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(working_file, err));
        }
        if let Err(err) = writer.into_inner().into_std().await.close() {
            let _ = tokio::fs::remove_file(&working_file).await;
            return Err(Error::StorageWriteError(working_file.clone(), err));
        }
        self.objects.ensure_base_dir(&filepath)?;
        match tokio::fs::rename(&working_file, &filepath).await {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = tokio::fs::remove_file(&working_file).await;
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(Error::StorageWriteError(filepath, err)),
                }
            }
        }
    }

    async fn remove_object(&self, digest: encoding::Digest) -> crate::Result<()> {
        let filepath = self.objects.build_digest_path(&digest);

        // this might fail but we don't consider that fatal just yet
        let _ = tokio::fs::set_permissions(&filepath, std::fs::Permissions::from_mode(0o777)).await;

        if let Err(err) = tokio::fs::remove_file(&filepath).await {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(Error::StorageWriteError(filepath, err)),
            };
        }
        Ok(())
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> crate::Result<bool> {
        let filepath = self.objects.build_digest_path(&digest);

        // this might fail but we don't consider that fatal just yet
        let _ = tokio::fs::set_permissions(&filepath, std::fs::Permissions::from_mode(0o777)).await;

        let metadata = tokio::fs::symlink_metadata(&filepath)
            .await
            .map_err(|err| Error::StorageReadError(filepath.clone(), err))?;

        let mtime = metadata
            .modified()
            .map_err(|err| Error::StorageReadError(filepath.clone(), err))?;

        if DateTime::<Utc>::from(mtime) >= older_than {
            return Ok(false);
        }

        if let Err(err) = tokio::fs::remove_file(&filepath).await {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(true),
                _ => Err(Error::StorageWriteError(filepath, err)),
            };
        }
        Ok(true)
    }
}
