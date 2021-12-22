// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use tonic::{Request, Response, Status};

use crate::proto::{self, payload_service_server::PayloadServiceServer, RpcResult};
use crate::storage::{self, PayloadStorage};

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
        todo!()
    }

    async fn has_payload(
        &self,
        _request: Request<proto::HasPayloadRequest>,
    ) -> Result<Response<proto::HasPayloadResponse>, Status> {
        todo!()
    }

    async fn open_payload(
        &self,
        _request: Request<proto::OpenPayloadRequest>,
    ) -> Result<Response<proto::OpenPayloadResponse>, Status> {
        todo!()
    }

    async fn remove_payload(
        &self,
        _request: Request<proto::RemovePayloadRequest>,
    ) -> Result<Response<proto::RemovePayloadResponse>, Status> {
        todo!()
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
        Box::pin(futures::future::ready(
            hyper::Response::builder()
                .status(hyper::http::StatusCode::METHOD_NOT_ALLOWED)
                .body(hyper::Body::empty())
                .map_err(|e| crate::Error::String(e.to_string())),
        ))
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
