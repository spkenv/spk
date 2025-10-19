// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::ErrorKind;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::future::ready;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};

use super::{MaybeOpenFsRepository, OpenFsRepository};
use crate::storage::fs::database::remove_file_if_older_than;
use crate::tracking::BlobRead;
use crate::{Error, PayloadError, PayloadResult, encoding};

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for MaybeOpenFsRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let Ok(opened) = self.opened().await else {
            return false;
        };
        opened.has_payload(digest).await
    }

    async fn payload_size(&self, digest: encoding::Digest) -> PayloadResult<u64> {
        let opened = self
            .opened()
            .await
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))?;
        opened.payload_size(digest).await
    }

    fn iter_payload_digests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = PayloadResult<encoding::Digest>> + Send>> {
        self.opened()
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))
            .and_then(|opened| ready(Ok(opened.iter_payload_digests())))
            .try_flatten_stream()
            .boxed()
    }

    async fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> PayloadResult<(encoding::Digest, u64)> {
        let opened = self
            .opened()
            .await
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))?;
        opened.write_data(reader).await
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> PayloadResult<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        self.opened()
            .await
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))?
            .open_payload(digest)
            .await
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> PayloadResult<()> {
        self.opened()
            .await
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))?
            .remove_payload(digest)
            .await
    }

    async fn remove_payload_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> PayloadResult<bool> {
        self.opened()
            .await
            .map_err(|err| PayloadError::String(format!("repository failed to open: {err}")))?
            .remove_payload_if_older_than(older_than, digest)
            .await
    }
}

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for OpenFsRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let path = self.payloads.build_digest_path(&digest);
        tokio::fs::symlink_metadata(path).await.is_ok()
    }

    async fn payload_size(&self, digest: encoding::Digest) -> PayloadResult<u64> {
        let path = self.payloads.build_digest_path(&digest);
        tokio::fs::symlink_metadata(&path)
            .await
            .map(|meta| meta.len())
            .map_err(|err| match err.kind() {
                ErrorKind::NotFound => PayloadError::UnknownPayload(digest),
                _ => {
                    PayloadError::StorageReadError("symlink_metadata on payload", path, err.into())
                }
            })
    }

    fn iter_payload_digests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = PayloadResult<encoding::Digest>> + Send>> {
        Box::pin(
            self.payloads
                .iter()
                .map_err(|err| PayloadError::String(format!("failed to iterate payloads: {err}"))),
        )
    }

    async fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> PayloadResult<(encoding::Digest, u64)> {
        self.payloads
            .write_data(reader)
            .await
            .map_err(|err| PayloadError::String(format!("failed to write payload data: {err}")))
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> PayloadResult<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::File::open(&path).await {
            Ok(file) => Ok((Box::pin(tokio::io::BufReader::new(file)), path)),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(PayloadError::UnknownPayload(digest)),
                _ => Err(PayloadError::StorageReadError(
                    "open on payload",
                    path,
                    err.into(),
                )),
            },
        }
    }

    // TODO: This operation is unsafe, a payload must not be removed if _any_
    // reference to it still exists, which could be in Manifests, Layer
    // annotations, tags (can you tag a blob directly?), etc.
    async fn remove_payload(&self, digest: encoding::Digest) -> PayloadResult<()> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(PayloadError::UnknownPayload(digest)),
                _ => Err(PayloadError::StorageWriteError(
                    "remove_file on payload",
                    path,
                    err.into(),
                )),
            },
        }
    }

    async fn remove_payload_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> PayloadResult<bool> {
        let filepath = self.payloads.build_digest_path(&digest);
        remove_file_if_older_than(older_than, &filepath, digest)
            .await
            .map_err(|err| match err {
                Error::UnknownObject(digest) => PayloadError::UnknownPayload(digest),
                Error::StorageReadError(op, path, source) => {
                    PayloadError::StorageReadError(op, path, source.into())
                }
                Error::StorageWriteError(op, path, source) => {
                    PayloadError::StorageWriteError(op, path, source.into())
                }
                err => PayloadError::String(format!("failed to remove payload: {err}")),
            })
    }
}
