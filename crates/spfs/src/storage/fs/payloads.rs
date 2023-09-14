// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::ErrorKind;
use std::pin::Pin;

use futures::future::ready;
use futures::{Stream, StreamExt, TryFutureExt};

use super::{FsRepository, OpenFsRepository};
use crate::storage::prelude::*;
use crate::tracking::BlobRead;
use crate::{encoding, Error, Result};

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for FsRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let Ok(opened) = self.opened().await else {
            return false;
        };
        opened.has_payload(digest).await
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.opened()
            .and_then(|opened| ready(Ok(opened.iter_payload_digests())))
            .try_flatten_stream()
            .boxed()
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        let opened = self.opened().await?;
        // Safety: we are simply deferring this function to the inner
        // one and so the same safety rules apply to our caller
        unsafe { opened.write_data(reader).await }
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

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        Box::pin(self.payloads.iter())
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
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
                ErrorKind::NotFound => {
                    // Return an error specific to this situation, whether the
                    // blob is really unknown or just the payload is missing.
                    match self.read_blob(digest).await {
                        Ok(blob) => Err(Error::ObjectMissingPayload(blob.into(), digest)),
                        Err(err @ Error::ObjectNotABlob(_, _)) => Err(err),
                        Err(_) => Err(Error::UnknownObject(digest)),
                    }
                }
                _ => Err(Error::StorageReadError("open on payload", path, err)),
            },
        }
    }

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
