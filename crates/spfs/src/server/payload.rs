// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use prost::Message;
use tonic::{Request, Response, Status};

use crate::prelude::*;
use crate::proto::payload_service_server::PayloadServiceServer;
use crate::proto::{self, RpcResult, convert_digest};
use crate::storage;

/// The payload service is both a gRPC service AND an http server
///
/// The grpc portion handles payload-related requests as expected,
/// but defers actual upload and download of file data to the http
/// server. This handoff is required because gRPC is really inefficient
/// at large file transfers. It is also a useful way to allow for
/// partitioning and/or migration of the underlying file storage in
/// the future
#[derive(Debug, Clone)]
pub struct PayloadService {
    repo: Arc<storage::RepositoryHandle>,
    external_root: url::Url,
}

#[tonic::async_trait]
impl proto::payload_service_server::PayloadService for PayloadService {
    type IterDigestsStream =
        Pin<Box<dyn Stream<Item = Result<proto::IterDigestsResponse, Status>> + Send>>;

    async fn iter_digests(
        &self,
        _request: Request<proto::IterDigestsRequest>,
    ) -> Result<Response<Self::IterDigestsStream>, Status> {
        let stream = self
            .repo
            .iter_payload_digests()
            .map(proto::IterDigestsResponse::from_result)
            .map(Ok);
        let stream: Self::IterDigestsStream = Box::pin(stream);
        let response = Response::new(stream);
        Ok(response)
    }

    async fn write_payload(
        &self,
        _request: Request<proto::WritePayloadRequest>,
    ) -> Result<Response<proto::WritePayloadResponse>, Status> {
        let data = proto::write_payload_response::UploadOption {
            url: self.external_root.to_string(),
        };
        let result = proto::WritePayloadResponse::ok(data);
        Ok(Response::new(result))
    }

    async fn has_payload(
        &self,
        request: Request<proto::HasPayloadRequest>,
    ) -> Result<Response<proto::HasPayloadResponse>, Status> {
        let request = request.into_inner();
        let digest = convert_digest(request.digest)
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let exists = self.repo.has_payload(digest).await;
        let result = proto::HasPayloadResponse { exists };
        Ok(Response::new(result))
    }

    async fn open_payload(
        &self,
        request: Request<proto::OpenPayloadRequest>,
    ) -> Result<Response<proto::OpenPayloadResponse>, Status> {
        let request = request.into_inner();
        let digest: crate::encoding::Digest = proto::handle_error!(convert_digest(request.digest));
        // do a little effort to determine if we can actually serve the
        // requested payload
        let _ = proto::handle_error!(self.repo.open_payload(digest).await);
        let mut option = proto::open_payload_response::DownloadOption::default();
        let mut self_download = self.external_root.clone();
        if let Ok(mut p) = self_download.path_segments_mut() {
            p.push(&digest.to_string());
        }
        option.locations.push(self_download.into());
        let result = proto::OpenPayloadResponse::ok(option);
        Ok(Response::new(result))
    }

    async fn remove_payload(
        &self,
        request: Request<proto::RemovePayloadRequest>,
    ) -> Result<Response<proto::RemovePayloadResponse>, Status> {
        let request = request.into_inner();
        let digest: crate::encoding::Digest = proto::handle_error!(convert_digest(request.digest));
        proto::handle_error!(self.repo.remove_payload(digest).await);
        let result = proto::RemovePayloadResponse::ok(proto::Ok {});
        Ok(Response::new(result))
    }
}

impl<B> hyper::service::Service<hyper::http::Request<B>> for PayloadService
where
    B: hyper::body::Body + Send + Sync + 'static,
    B::Error: std::error::Error,
    B::Data: AsRef<[u8]> + Send + Sync,
{
    type Response = hyper::http::Response<ResponseBody>;
    type Error = crate::Error;
    type Future =
        std::pin::Pin<Box<dyn futures::Future<Output = crate::Result<Self::Response>> + Send>>;

    fn call(&self, req: hyper::http::Request<B>) -> Self::Future {
        match *req.method() {
            hyper::Method::POST => Box::pin(handle_upload(self.repo.clone(), req)),
            hyper::Method::GET => Box::pin(handle_download(self.repo.clone(), req)),
            _ => Box::pin(futures::future::ready(
                hyper::Response::builder()
                    .status(hyper::http::StatusCode::METHOD_NOT_ALLOWED)
                    .body(http_body_util::StreamBody::new(FramedReader::default()))
                    .map_err(|e| crate::Error::String(e.to_string())),
            )),
        }
    }
}

impl PayloadService {
    pub fn new(repo: Arc<storage::RepositoryHandle>, external_root: url::Url) -> Self {
        Self {
            repo,
            external_root,
        }
    }

    pub fn new_srv(
        repo: Arc<storage::RepositoryHandle>,
        external_root: url::Url,
    ) -> PayloadServiceServer<Self> {
        Self::new(repo, external_root).into_srv()
    }

    pub fn into_srv(self) -> PayloadServiceServer<Self> {
        PayloadServiceServer::new(self)
    }
}

async fn handle_upload<B>(
    repo: Arc<storage::RepositoryHandle>,
    mut req: hyper::http::Request<B>,
) -> crate::Result<hyper::Response<ResponseBody>>
where
    B: hyper::body::Body + Send + Sync + 'static,
    B::Error: std::error::Error,
    B::Data: AsRef<[u8]> + Send + Sync,
{
    let content_type = req.headers_mut().remove(hyper::http::header::CONTENT_TYPE);
    let reader = body_to_reader(req.into_body());
    match content_type.as_ref().map(|v| v.to_str()) {
        None | Some(Ok("application/octet-stream")) => {
            let reader = Box::pin(reader);
            handle_uncompressed_upload(repo, reader).await
        }
        Some(Ok("application/x-bzip2")) => {
            let reader = async_compression::tokio::bufread::BzDecoder::new(reader);
            let reader = Box::pin(tokio::io::BufReader::new(reader));
            handle_uncompressed_upload(repo, reader).await
        }
        _ => hyper::http::Response::builder()
            .status(hyper::http::StatusCode::UNSUPPORTED_MEDIA_TYPE)
            .body(http_body_util::StreamBody::new(FramedReader::from(
                "Invalid or unsupported Content-Type",
            )))
            .map_err(|e| crate::Error::String(e.to_string())),
    }
}

async fn handle_uncompressed_upload(
    repo: Arc<storage::RepositoryHandle>,
    reader: Pin<Box<dyn crate::tracking::BlobRead>>,
) -> crate::Result<hyper::Response<ResponseBody>> {
    let result = repo.write_data(reader).await;
    let (digest, size) = result.map_err(|err| {
        crate::Error::String(format!(
            "An error occurred while spawning a thread for this operation: {err:?}"
        ))
    })?;
    let result = crate::proto::write_payload_response::UploadResponse::ok(
        crate::proto::write_payload_response::upload_response::UploadResult {
            digest: Some(digest.into()),
            size,
        },
    );
    let bytes = result.encode_to_vec();
    hyper::Response::builder()
        .status(hyper::http::StatusCode::OK)
        .body(http_body_util::StreamBody::new(FramedReader::from(bytes)))
        .map_err(|e| crate::Error::String(e.to_string()))
}

fn body_to_reader<B>(body: B) -> Pin<Box<impl crate::tracking::BlobRead>>
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

async fn handle_download<B>(
    repo: Arc<storage::RepositoryHandle>,
    mut req: hyper::http::Request<B>,
) -> crate::Result<hyper::http::Response<ResponseBody>>
where
    B: hyper::body::Body + Send + Sync + 'static,
    B::Error: std::error::Error,
    B::Data: AsRef<[u8]> + Send + Sync,
{
    let relative_path = req.uri().path().trim_start_matches('/');
    let digest = crate::encoding::Digest::parse(relative_path)?;
    let (uncompressed_reader, _) = repo.open_payload(digest).await?;
    let accepted = req
        .headers_mut()
        .get_all(hyper::http::header::ACCEPT)
        .into_iter();
    let get_body_and_content_type = move || -> (FramedReader, hyper::http::HeaderValue) {
        for accepted in accepted {
            match accepted.to_str() {
                Ok("application/octet-stream") => {
                    // this is the default, uncompressed
                    break;
                }
                Ok("application/x-bzip2") => {
                    return (
                        FramedReader::from(async_compression::tokio::bufread::BzEncoder::new(
                            uncompressed_reader,
                        )),
                        accepted.to_owned(),
                    );
                }
                _ => continue,
            }
        }
        (
            FramedReader::from(uncompressed_reader),
            hyper::http::HeaderValue::from_static("application/octet-stream"),
        )
    };
    let (stream, content_type) = get_body_and_content_type();
    hyper::Response::builder()
        .status(hyper::http::StatusCode::OK)
        .header(hyper::http::header::CONTENT_TYPE, content_type)
        .body(http_body_util::StreamBody::new(stream))
        .map_err(|e| crate::Error::String(e.to_string()))
}

/// The body of the response to a payload upload or download request
type ResponseBody = http_body_util::StreamBody<FramedReader>;

pub struct FramedReader {
    inner: tokio_util::io::ReaderStream<Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>>,
}

impl Default for FramedReader {
    fn default() -> Self {
        Self::from("")
    }
}

impl From<&'static str> for FramedReader {
    fn from(value: &'static str) -> Self {
        Self {
            inner: tokio_util::io::ReaderStream::new(Box::pin(std::io::Cursor::new(value))),
        }
    }
}

impl From<Vec<u8>> for FramedReader {
    fn from(value: Vec<u8>) -> Self {
        Self {
            inner: tokio_util::io::ReaderStream::new(Box::pin(std::io::Cursor::new(value))),
        }
    }
}

impl From<Pin<Box<dyn BlobRead>>> for FramedReader {
    fn from(value: Pin<Box<dyn BlobRead>>) -> Self {
        Self {
            inner: tokio_util::io::ReaderStream::new(value),
        }
    }
}

impl<T> From<async_compression::tokio::bufread::BzEncoder<T>> for FramedReader
where
    async_compression::tokio::bufread::BzEncoder<T>: tokio::io::AsyncRead + Send + Sync + 'static,
{
    fn from(value: async_compression::tokio::bufread::BzEncoder<T>) -> Self {
        Self {
            inner: tokio_util::io::ReaderStream::new(Box::pin(value)),
        }
    }
}

impl Stream for FramedReader {
    type Item = Result<hyper::body::Frame<bytes::Bytes>, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context,
    ) -> std::task::Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(data))) => {
                let frame = hyper::body::Frame::data(data);
                std::task::Poll::Ready(Some(Ok(frame)))
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Ready(Some(Err(err))) => std::task::Poll::Ready(Some(Err(err))),
        }
    }
}
