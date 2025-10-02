// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;
use std::pin::Pin;

use futures::{Stream, TryStreamExt};
use prost::Message;

use crate::proto::{self, RpcResult};
use crate::tracking::BlobRead;
use crate::{Error, Result, encoding, storage};

#[async_trait::async_trait]
impl storage::PayloadStorage for super::RpcRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        let request = proto::HasPayloadRequest {
            digest: Some(digest.into()),
        };
        self.payload_client
            .clone()
            .has_payload(request)
            .await
            .ok()
            .map(|resp| resp.into_inner().exists)
            .unwrap_or(false)
    }

    async fn payload_size(&self, digest: encoding::Digest) -> Result<u64> {
        let request = proto::PayloadSizeRequest {
            digest: Some(digest.into()),
        };
        let response = self
            .payload_client
            .clone()
            .payload_size(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(response)
    }

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

    async fn write_data(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<(encoding::Digest, u64)> {
        let request = proto::WritePayloadRequest {};
        let option = self
            .payload_client
            .clone()
            .write_payload(request)
            .await?
            .into_inner()
            .to_result()?;
        let compressed_reader = async_compression::tokio::bufread::BzEncoder::new(reader);
        let stream = tokio_util::io::ReaderStream::new(compressed_reader);
        let stream_body = http_body_util::StreamBody::new(stream.map_ok(hyper::body::Frame::data));
        let request = hyper::Request::builder()
            .method(hyper::Method::POST)
            .header(hyper::http::header::CONTENT_TYPE, "application/x-bzip2")
            .uri(&option.url)
            .body(stream_body)
            .map_err(|err| {
                crate::Error::String(format!("Failed to build upload request: {err:?}"))
            })?;
        let resp = self.send_http_request(request).await?;
        if !resp.status().is_success() {
            // the server is expected to return all errors via the gRPC message
            // payload in the body. Any other status code is unexpected
            return Err(crate::Error::String(format!(
                "Unexpected status code from payload server: {}",
                resp.status()
            )));
        }
        let stream = http_body_util::BodyDataStream::new(resp.into_body());
        let bytes = stream
            .try_fold(Vec::new(), |mut data, chunk| async move {
                data.extend_from_slice(&chunk);
                Ok(data)
            })
            .await
            .map_err(|err| format!("Failed to read response from payload server: {err:?}"))?;
        let result = crate::proto::write_payload_response::UploadResponse::decode(bytes.as_slice())
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
        let url_str = option
            .locations
            .first()
            .ok_or_else(|| crate::Error::String("upload option gave no locations to try".into()))?;
        let req = hyper::Request::builder()
            .uri(url_str)
            .method(hyper::http::Method::GET)
            .header(hyper::http::header::ACCEPT, "application/x-bzip2")
            .header(hyper::http::header::ACCEPT, "application/octet-stream")
            .body(http_body_util::Empty::<hyper::body::Bytes>::new())
            .map_err(|err| {
                crate::Error::String(format!("Failed to build download request: {err:?}"))
            })?;
        let resp = self.send_http_request(req).await?;
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

impl super::RpcRepository {
    async fn send_http_request<B>(
        &self,
        request: hyper::Request<B>,
    ) -> Result<hyper::Response<hyper::body::Incoming>>
    where
        B: hyper::body::Body + Send + Sync + 'static,
        B::Data: Send + Sync,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let host = request.uri().host().ok_or_else(|| {
            Error::new(format!(
                "missing valid host in request uri, got {}",
                request.uri()
            ))
        })?;
        let port = request.uri().port_u16().unwrap_or(80);
        let address = format!("{host}:{port}");
        tracing::trace!("Connecting to remote repository at {address}");
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|err| Error::new(format!("failed to connect to remote repository: {err}")))?;
        let io = hyper_util::rt::TokioIo::new(stream);
        let (mut sender, conn) = self.http_client.handshake(io).await.map_err(|err| {
            Error::new(format!(
                "Failed to establish connection with remote repository: {err}"
            ))
        })?;
        tokio::spawn(conn);
        sender
            .send_request(request)
            .await
            .map_err(|err| crate::Error::String(format!("Failed to send http request: {err}")))
    }
}

fn open_download_stream(
    mut resp: hyper::http::Response<hyper::body::Incoming>,
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

fn body_to_reader<B>(body: B) -> Pin<Box<impl BlobRead>>
where
    B: hyper::body::Body + Send + Sync + 'static,
    B::Error: std::error::Error,
    B::Data: AsRef<[u8]> + Send + Sync,
{
    // the stream must return io errors in order to be converted to a reader
    let mapped_stream = http_body_util::BodyDataStream::new(body)
        .map_err(|err| std::io::Error::other(format!("Failed to read response body: {err:?}")))
        .into_async_read();
    let stream_reader = tokio_util::compat::FuturesAsyncReadCompatExt::compat(mapped_stream);
    let buffered_reader = tokio::io::BufReader::new(stream_reader);
    Box::pin(buffered_reader)
}
