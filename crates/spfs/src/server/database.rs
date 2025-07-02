// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::prelude::*;
use crate::proto::database_service_server::DatabaseServiceServer;
use crate::proto::{self, RpcResult, convert_digest, convert_to_datetime};
use crate::storage;

#[derive(Debug, Clone)]
pub struct DatabaseService {
    repo: Arc<storage::RepositoryHandle>,
}

#[tonic::async_trait]
impl proto::database_service_server::DatabaseService for DatabaseService {
    type FindDigestsStream =
        Pin<Box<dyn Stream<Item = Result<proto::FindDigestsResponse, Status>> + Send>>;
    type IterObjectsStream =
        tokio_stream::Iter<std::vec::IntoIter<Result<proto::IterObjectsResponse, Status>>>;
    type WalkObjectsStream =
        tokio_stream::Iter<std::vec::IntoIter<Result<proto::WalkObjectsResponse, Status>>>;

    async fn has_object(
        &self,
        request: Request<proto::HasObjectRequest>,
    ) -> Result<Response<proto::HasObjectResponse>, Status> {
        let request = request.into_inner();
        let digest = convert_digest(request.digest)
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        Ok(Response::new(proto::HasObjectResponse {
            exists: self.repo.has_object(digest).await,
        }))
    }

    async fn read_object(
        &self,
        request: Request<proto::ReadObjectRequest>,
    ) -> Result<Response<proto::ReadObjectResponse>, Status> {
        let request = request.into_inner();
        let digest = proto::handle_error!(convert_digest(request.digest));
        let object = { proto::handle_error!(self.repo.read_object(digest).await) };
        let result = proto::ReadObjectResponse::ok((&object).into());
        Ok(Response::new(result))
    }

    async fn find_digests(
        &self,
        request: Request<proto::FindDigestsRequest>,
    ) -> Result<Response<Self::FindDigestsStream>, Status> {
        let request = request.into_inner();
        let search_criteria = request
            .search_criteria
            .try_into()
            .map_err(|err: crate::Error| Status::invalid_argument(err.to_string()))?;
        let stream = self
            .repo
            .find_digests(search_criteria)
            .map(proto::FindDigestsResponse::from_result)
            .map(Ok);
        let stream: Self::FindDigestsStream = Box::pin(stream);
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
        let digest: crate::encoding::Digest = proto::handle_error!(convert_digest(request.digest));
        proto::handle_error!(self.repo.remove_object(digest).await);
        let result = proto::RemoveObjectResponse::ok(proto::Ok {});
        Ok(Response::new(result))
    }

    async fn remove_object_if_older_than(
        &self,
        request: Request<proto::RemoveObjectIfOlderThanRequest>,
    ) -> Result<Response<proto::RemoveObjectIfOlderThanResponse>, Status> {
        let request = request.into_inner();
        let older_than: DateTime<Utc> =
            proto::handle_error!(convert_to_datetime(request.older_than));
        let digest: crate::encoding::Digest = proto::handle_error!(convert_digest(request.digest));
        let deleted = proto::handle_error!(
            self.repo
                .remove_object_if_older_than(older_than, digest)
                .await
        );
        let result = proto::RemoveObjectIfOlderThanResponse::ok(deleted);
        Ok(Response::new(result))
    }
}

impl DatabaseService {
    pub fn new(repo: Arc<storage::RepositoryHandle>) -> Self {
        Self { repo }
    }

    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> DatabaseServiceServer<Self> {
        DatabaseServiceServer::new(Self::new(repo))
    }
}
