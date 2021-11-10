// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use super::proto;
use crate::storage;
use proto::tag_service_server::TagServiceServer;

#[derive(Debug, Clone)]
pub struct TagService {
    repo: Arc<storage::RepositoryHandle>,
}

#[tonic::async_trait]
impl proto::tag_service_server::TagService for TagService {
    async fn ls_tags(
        &self,
        request: Request<proto::LsTagsRequest>,
    ) -> std::result::Result<Response<proto::LsTagsResponse>, Status> {
        tracing::trace!("recieve request");
        let request = request.into_inner();
        let path = relative_path::RelativePath::new(&request.path);
        let entries: crate::Result<Vec<_>> = {
            self.repo.ls_tags(path).collect().await
        };

        let data = proto::LsTagsResponse {
            entries: entries.unwrap(),
        };
        Ok(Response::new(data))
    }

    async fn resolve_tag(
        &self,
        request: tonic::Request<proto::ResolveTagRequest>,
    ) -> Result<tonic::Response<proto::ResolveTagResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn find_tags(
        &self,
        request: tonic::Request<proto::FindTagsRequest>,
    ) -> Result<tonic::Response<proto::FindTagsResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn iter_tag_specs(
        &self,
        request: tonic::Request<proto::IterTagSpecsRequest>,
    ) -> Result<tonic::Response<proto::IterTagSpecsResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn read_tag(
        &self,
        request: tonic::Request<proto::ReadTagRequest>,
    ) -> Result<tonic::Response<proto::ReadTagResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn push_raw_tag(
        &self,
        request: tonic::Request<proto::PushRawTagRequest>,
    ) -> Result<tonic::Response<proto::PushRawTagResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn remove_tag_stream(
        &self,
        request: tonic::Request<proto::RemoveTagStreamRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagStreamResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }

    async fn remove_tag(
        &self,
        request: tonic::Request<proto::RemoveTagRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagResponse>, tonic::Status> {
        let _request = request.into_inner();
        todo!()
    }
}

impl TagService {
    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> TagServiceServer<Self> {
        TagServiceServer::new(Self { repo })
    }
}
