// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use prost::Message;
use tonic::{Request, Response, Status};

use crate::proto::{self, convert_digest, payload_service_server::PayloadServiceServer, RpcResult};
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
        let digest: crate::encoding::Digest = proto::handle_error!(convert_digest(request.digest));
        let exists = self.repo.has_payload(digest).await;
        let result = proto::HasPayloadResponse::ok(exists);
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

impl hyper::service::Service<hyper::http::Request<hyper::Body>> for PayloadService {
    type Response = hyper::http::Response<hyper::Body>;
    type Error = crate::Error;
    type Future =
        std::pin::Pin<Box<dyn futures::Future<Output = crate::Result<Self::Response>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: hyper::http::Request<hyper::Body>) -> Self::Future {
        match *req.method() {
            hyper::Method::POST => Box::pin(handle_upload(self.repo.clone(), req.into_body())),
            hyper::Method::GET => Box::pin(handle_download(
                self.repo.clone(),
                req.uri().path().trim_start_matches('/').to_string(),
            )),
            _ => Box::pin(futures::future::ready(
                hyper::Response::builder()
                    .status(hyper::http::StatusCode::METHOD_NOT_ALLOWED)
                    .body(hyper::Body::empty())
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

async fn handle_upload(
    repo: Arc<storage::RepositoryHandle>,
    body: hyper::Body,
) -> crate::Result<hyper::http::Response<hyper::Body>> {
    let reader = body_to_reader(body);
    // Safety: it is unsafe to create a payload without it's corresponding
    // blob, but this payload http server is part of a larger repository
    // and does not intend to be responsible for ensuring the integrity
    // of the object graph - only the up/down of payload data
    let result = unsafe { repo.write_data(reader).await };
    let (digest, size) = result.map_err(|err| {
        crate::Error::String(format!(
            "An error occurred while spawning a thread for this operation: {:?}",
            err
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
        .body(bytes.into())
        .map_err(|e| crate::Error::String(e.to_string()))
}

fn body_to_reader(body: hyper::Body) -> Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>> {
    Box::pin(tokio_util::io::StreamReader::new(body.map(|chunk| {
        chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    })))
}

async fn handle_download(
    repo: Arc<storage::RepositoryHandle>,
    relative_path: String,
) -> crate::Result<hyper::http::Response<hyper::Body>> {
    let digest = crate::encoding::Digest::parse(&relative_path)?;
    let (reader, _) = repo.open_payload(digest).await?;
    hyper::Response::builder()
        .status(hyper::http::StatusCode::OK)
        .body(hyper::Body::wrap_stream(tokio_util::io::ReaderStream::new(
            reader,
        )))
        .map_err(|e| crate::Error::String(e.to_string()))
}
