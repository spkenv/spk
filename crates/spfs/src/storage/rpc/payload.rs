// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::pin::Pin;

use futures::Stream;
use prost::Message;

use crate::{
    encoding,
    proto::{self, RpcResult},
    storage, Result,
};

#[async_trait::async_trait]
impl storage::PayloadStorage for super::RpcRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        todo!()
    }

    async fn write_data(
        &self,
        reader: Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>,
    ) -> Result<(encoding::Digest, u64)> {
        let request = proto::WritePayloadRequest {};
        let option = self
            .payload_client
            .clone()
            .write_payload(request)
            .await?
            .into_inner()
            .to_result()?;
        let client = reqwest::Client::new();
        tracing::warn!("{}", option.url);
        let stream =
            tokio_util::codec::FramedRead::new(reader, tokio_util::codec::BytesCodec::new());
        let resp = client
            .post(&option.url)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .expect("failed to send request")
            .error_for_status()
            .expect("Failed request");
        if !resp.status().is_success() {
            panic!("{:?}", resp.status());
        }
        let bytes = resp.bytes().await.expect("could not read body bytes");
        tracing::warn!("{:?}", bytes);
        let result = crate::proto::write_payload_response::UploadResponse::decode(bytes)
            .expect("Invalid response data")
            .to_result()?;
        Ok((result.digest.try_into()?, result.size))
    }

    async fn open_payload(
        &self,
        _digest: encoding::Digest,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>> {
        todo!()
    }

    async fn remove_payload(&self, _digest: encoding::Digest) -> Result<()> {
        todo!()
    }
}
