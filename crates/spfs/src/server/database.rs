// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::proto::{self, database_service_server::DatabaseServiceServer, RpcResult};
use crate::storage;

#[derive(Debug, Clone)]
pub struct DatabaseService {
    repo: Arc<storage::RepositoryHandle>,
}

#[tonic::async_trait]
impl proto::database_service_server::DatabaseService for DatabaseService {
    type IterDigestsStream =
        Pin<Box<dyn Stream<Item = Result<proto::IterDigestsResponse, Status>> + Send>>;
    type IterObjectsStream =
        tokio_stream::Iter<std::vec::IntoIter<Result<proto::IterObjectsResponse, Status>>>;
    type WalkObjectsStream =
        tokio_stream::Iter<std::vec::IntoIter<Result<proto::WalkObjectsResponse, Status>>>;

    async fn read_object(
        &self,
        request: Request<proto::ReadObjectRequest>,
    ) -> Result<Response<proto::ReadObjectResponse>, Status> {
        let request = request.into_inner();
        let digest = proto::handle_error!(request.digest.try_into());
        let object = { proto::handle_error!(self.repo.read_object(digest).await) };
        let result = proto::ReadObjectResponse::ok((&object).into());
        Ok(Response::new(result))
    }

    async fn iter_digests(
        &self,
        _request: Request<proto::IterDigestsRequest>,
    ) -> Result<Response<Self::IterDigestsStream>, Status> {
        let stream = self
            .repo
            .iter_digests()
            .map(proto::IterDigestsResponse::from_result)
            .map(Ok);
        let stream: Self::IterDigestsStream = Box::pin(stream);
        let response = Response::new(stream);
        Ok(response)
    }

    async fn iter_objects(
        &self,
        _request: Request<proto::IterObjectsRequest>,
    ) -> Result<Response<Self::IterObjectsStream>, Status> {
        Err(Status::unimplemented(
            "object iteration is no yet supported directly over gRPC",
        ))
    }

    async fn walk_objects(
        &self,
        _request: Request<proto::WalkObjectsRequest>,
    ) -> Result<Response<Self::WalkObjectsStream>, Status> {
        Err(Status::unimplemented(
            "object walking is no yet supported directly over gRPC",
        ))
    }

    async fn write_object(
        &self,
        request: Request<proto::WriteObjectRequest>,
    ) -> Result<Response<proto::WriteObjectResponse>, Status> {
        let request = request.into_inner();
        let object = proto::handle_error!(request.object.try_into());
        {
            proto::handle_error!(self.repo.write_object(&object).await)
        };
        let result = proto::WriteObjectResponse::ok(proto::Ok {});
        Ok(Response::new(result))
    }

    async fn remove_object(
        &self,
        request: Request<proto::RemoveObjectRequest>,
    ) -> Result<Response<proto::RemoveObjectResponse>, Status> {
        let request = request.into_inner();
        let digest: crate::encoding::Digest = proto::handle_error!(request.digest.try_into());
        proto::handle_error!(self.repo.remove_object(digest).await);
        let result = proto::RemoveObjectResponse::ok(proto::Ok {});
        Ok(Response::new(result))
    }
}

impl DatabaseService {
    pub fn new(repo: Arc<storage::RepositoryHandle>) -> Self {
        Self{repo}
    }

    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> DatabaseServiceServer<Self> {
        DatabaseServiceServer::new(Self::new(repo))
    }
}
