// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use crate::{encoding, storage, Result};

#[async_trait::async_trait]
impl storage::PayloadStorage for super::RpcRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        todo!()
    }

    async fn write_data(
        &self,
        _reader: Pin<Box<dyn tokio::io::AsyncRead + Send + 'static>>,
    ) -> Result<(encoding::Digest, u64)> {
        todo!()
    }

    async fn open_payload(
        &self,
        _digest: encoding::Digest,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + 'static>>> {
        todo!()
    }

    async fn remove_payload(&self, _digest: encoding::Digest) -> Result<()> {
        todo!()
    }
}
