// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::pin::Pin;

use futures::{Stream, StreamExt, TryStreamExt};
use prost::Message;

use crate::proto::{self, RpcResult};
use crate::tracking::BlobRead;
use crate::{encoding, storage, Error, Result};

#[async_trait::async_trait]
impl storage::PayloadStorage for super::RpcRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        let request = proto::IterDigestsRequest {};
        let mut client = self.payload_client.clone();
        let stream = futures::stream::once(async move { client.iter_digests(request).await })
            .map_err(crate::Error::from)
            .map_ok(|r| r.into_inner().map_err(crate::Error::from))
            .try_flatten()
            .and_then(|d| async { d.to_result() })
            .and_then(|d| async { d.try_into() });
        Box::pin(stream)
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        let request = proto::WritePayloadRequest {};
        let option = self
            .payload_client
            .clone()
            .write_payload(request)
            .await?
            .into_inner()
            .to_result()?;
        let client = hyper::Client::new();
        let compressed_reader = async_compression::tokio::bufread::BzEncoder::new(reader);
        let stream = tokio_util::codec::FramedRead::new(
            compressed_reader,
            tokio_util::codec::BytesCodec::new(),
        );
        let request = hyper::Request::builder()
            .method(hyper::Method::POST)
            .header(hyper::http::header::CONTENT_TYPE, "application/x-bzip2")
            .uri(&option.url)
            .body(hyper::Body::wrap_stream(stream))
            .map_err(|err| {
                crate::Error::String(format!("Failed to build upload request: {err:?}"))
            })?;
        let resp = client.request(request).await.map_err(|err| {
            crate::Error::String(format!("Failed to send upload request: {err:?}"))
        })?;
        if !resp.status().is_success() {
            // the server is expected to return all errors via the gRPC message
            // payload in the body. Any other status code is unexpected
            return Err(crate::Error::String(format!(
                "Unexpected status code from payload server: {}",
                resp.status()
            )));
        }
        let bytes = hyper::body::to_bytes(resp.into_body())
            .await
            .map_err(|err| format!("Failed to read response from payload server: {err:?}"))?;
        let result = crate::proto::write_payload_response::UploadResponse::decode(bytes)
            .map_err(|err| format!("Payload server returned invalid response data: {err:?}"))?
            .to_result()?;
        Ok((proto::convert_digest(result.digest)?, result.size))
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
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
        let client = hyper::Client::new();
        let url_str = option
            .locations
            .get(0)
            .ok_or_else(|| crate::Error::String("upload option gave no locations to try".into()))?;
        let req = hyper::Request::builder()
            .uri(url_str)
            .method(hyper::http::Method::GET)
            .header(hyper::http::header::ACCEPT, "application/x-bzip2")
            .header(hyper::http::header::ACCEPT, "application/octet-stream")
            .body(hyper::Body::empty())
            .map_err(|err| {
                crate::Error::String(format!("Failed to build download request: {err:?}"))
            })?;
        let resp = client.request(req).await.map_err(|err| {
            crate::Error::String(format!("Failed to send download request: {err:?}"))
        })?;
        if !resp.status().is_success() {
            // the server is expected to return all errors via the gRPC message
            // payload in the body. Any other status code is unexpected
            return Err(crate::Error::String(format!(
                "Unexpected status code from payload server: {}",
                resp.status()
            )));
        }
        let stream = open_download_stream(resp)?;
        Ok((stream, url_str.into()))
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        let request = proto::RemovePayloadRequest {
            digest: Some(digest.into()),
        };
        self.payload_client
            .clone()
            .remove_payload(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }
}

fn open_download_stream(
    mut resp: hyper::http::Response<hyper::Body>,
) -> Result<Pin<Box<dyn BlobRead>>> {
    let content_type = resp.headers_mut().remove(hyper::http::header::CONTENT_TYPE);
    let reader = body_to_reader(resp.into_body());
    match content_type.as_ref().map(|v| v.to_str()) {
        None | Some(Ok("application/octet-stream")) => Ok(reader),
        Some(Ok("application/x-bzip2")) => {
            let reader = async_compression::tokio::bufread::BzDecoder::new(reader);
            Ok(Box::pin(tokio::io::BufReader::new(reader)))
        }
        _ => Err(Error::String(format!(
            "Invalid or unsupported Content-Type from the server: {content_type:?}"
        ))),
    }
}

fn body_to_reader(body: hyper::Body) -> Pin<Box<impl BlobRead>> {
    // the stream must return io errors in order to be converted to a reader
    let mapped_stream =
        body.map(|chunk| chunk.map_err(|e| futures::io::Error::new(std::io::ErrorKind::Other, e)));
    let stream_reader = tokio_util::io::StreamReader::new(mapped_stream);
    let buffered_reader = tokio::io::BufReader::new(stream_reader);
    Box::pin(buffered_reader)
}
