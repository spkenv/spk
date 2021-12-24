// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::pin::Pin;

use futures::{Stream, TryStreamExt};
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
        let stream =
            tokio_util::codec::FramedRead::new(reader, tokio_util::codec::BytesCodec::new());
        let resp = client
            .post(&option.url)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(|err| {
                crate::Error::String(format!("Failed to send upload request: {:?}", err))
            })?
            .error_for_status()
            .map_err(|err| crate::Error::String(format!("Upload failed: {:?}", err)))?;
        if !resp.status().is_success() {
            // the server is expected to return all errors via the gRPC message
            // payload in the body. Any other status code is unexpected
            return Err(crate::Error::String(format!(
                "Unexpected status code from payload server: {}",
                resp.status()
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|err| format!("Failed to read response from payload server: {:?}", err))?;
        let result = crate::proto::write_payload_response::UploadResponse::decode(bytes)
            .map_err(|err| format!("Payload server returned invalid response data: {:?}", err))?
            .to_result()?;
        Ok((result.digest.try_into()?, result.size))
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>> {
        let request = proto::OpenPayloadRequest {
            digest: Some(digest.into()),
        };
        let option = self
            .payload_client
            .clone()
            .open_payload(request)
            .await?
            .into_inner()
            .to_result()?;
        let client = reqwest::Client::new();
        let url = option.locations.get(0).map(String::as_str).unwrap_or("");
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|err| {
                crate::Error::String(format!("Failed to send download request: {:?}", err))
            })?
            .error_for_status()
            .map_err(|err| crate::Error::String(format!("Download failed: {:?}", err)))?;
        if !resp.status().is_success() {
            // the server is expected to return all errors via the gRPC message
            // payload in the body. Any other status code is unexpected
            return Err(crate::Error::String(format!(
                "Unexpected status code from payload server: {}",
                resp.status()
            )));
        }
        let stream = resp
            .bytes_stream()
            .map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e));
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Ok(Box::pin(stream.into_async_read().compat()))
    }

    async fn remove_payload(&self, _digest: encoding::Digest) -> Result<()> {
        todo!()
    }
}
