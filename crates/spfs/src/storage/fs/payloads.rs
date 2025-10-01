// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::ErrorKind;
use std::pin::Pin;

use futures::future::ready;
use futures::{Stream, StreamExt, TryFutureExt};

use super::{MaybeOpenFsRepository, OpenFsRepository};
use crate::tracking::BlobRead;
use crate::{Error, Result, encoding};

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for MaybeOpenFsRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let Ok(opened) = self.opened().await else {
            return false;
        };
        opened.has_payload(digest).await
    }

    async fn payload_size(&self, digest: encoding::Digest) -> Result<u64> {
        let opened = self.opened().await?;
        opened.payload_size(digest).await
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.opened()
            .and_then(|opened| ready(Ok(opened.iter_payload_digests())))
            .try_flatten_stream()
            .boxed()
    }

    async fn write_data(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<(encoding::Digest, u64)> {
        let opened = self.opened().await?;
        opened.write_data(reader).await
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        self.opened().await?.open_payload(digest).await
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        self.opened().await?.remove_payload(digest).await
    }
}

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for OpenFsRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let path = self.payloads.build_digest_path(&digest);
        tokio::fs::symlink_metadata(path).await.is_ok()
    }

    async fn payload_size(&self, digest: encoding::Digest) -> Result<u64> {
        let path = self.payloads.build_digest_path(&digest);
        tokio::fs::symlink_metadata(&path)
            .await
            .map(|meta| meta.len())
            .map_err(|err| match err.kind() {
                ErrorKind::NotFound => Error::UnknownObject(digest),
                _ => Error::StorageReadError("symlink_metadata on payload", path, err),
            })
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        Box::pin(self.payloads.iter())
    }

    async fn write_data(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<(encoding::Digest, u64)> {
        self.payloads.write_data(reader).await
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::File::open(&path).await {
            Ok(file) => Ok((Box::pin(tokio::io::BufReader::new(file)), path)),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(digest)),
                _ => Err(Error::StorageReadError("open on payload", path, err)),
            },
        }
    }

    // TODO: This operation is unsafe, a payload must not be removed if _any_
    // reference to it still exists, which could be in Manifests, Layer
    // annotations, tags (can you tag a blob directly?), etc.
    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(digest)),
                _ => Err(Error::StorageWriteError(
                    "remove_file on payload",
                    path,
                    err,
                )),
            },
        }
    }
}
