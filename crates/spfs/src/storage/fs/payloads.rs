// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{io::ErrorKind, pin::Pin};

use futures::Stream;

use super::FSRepository;
use crate::{encoding, Error, Result};

#[async_trait::async_trait]
impl crate::storage::PayloadStorage for FSRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>>>> {
        Box::pin(self.payloads.iter())
    }

    async fn write_data(
        &mut self,
        reader: Pin<Box<dyn tokio::io::AsyncRead + Send + 'static>>,
    ) -> Result<(encoding::Digest, u64)> {
        self.payloads.write_data(reader).await
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + 'static>>> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::File::open(&path).await {
            Ok(file) => Ok(Box::pin(file)),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(digest)),
                _ => Err(err.into()),
            },
        }
    }

    async fn remove_payload(&mut self, digest: encoding::Digest) -> Result<()> {
        let path = self.payloads.build_digest_path(&digest);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(digest)),
                _ => Err(err.into()),
            },
        }
    }
}
