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

#[derive(Debug, Clone)]
pub struct PayloadService {
    repo: Arc<storage::RepositoryHandle>,
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
        request: Request<proto::WritePayloadRequest>,
    ) -> Result<Response<proto::WritePayloadResponse>, Status> {
        todo!()
    }

    async fn has_payload(
        &self,
        request: Request<proto::HasPayloadRequest>,
    ) -> Result<Response<proto::HasPayloadResponse>, Status> {
        todo!()
    }

    async fn open_payload(
        &self,
        request: Request<proto::OpenPayloadRequest>,
    ) -> Result<Response<proto::OpenPayloadResponse>, Status> {
        todo!()
    }

    async fn remove_payload(
        &self,
        request: Request<proto::RemovePayloadRequest>,
    ) -> Result<Response<proto::RemovePayloadResponse>, Status> {
        todo!()
    }
}

impl PayloadService {
    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> PayloadServiceServer<Self> {
        PayloadServiceServer::new(Self { repo })
    }
}
