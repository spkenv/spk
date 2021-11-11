// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::sync::Arc;

use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::proto;
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
        let request = request.into_inner();
        let tag_spec = request.tag_spec.parse().unwrap();
        let tag = self.repo.resolve_tag(&tag_spec).await.unwrap();
        let data = proto::ResolveTagResponse {
            tag: Some((&tag).into()),
        };
        Ok(Response::new(data))
    }

    async fn find_tags(
        &self,
        request: tonic::Request<proto::FindTagsRequest>,
    ) -> Result<tonic::Response<proto::FindTagsResponse>, tonic::Status> {
        let request = request.into_inner();
        let digest = request.digest.try_into().unwrap();
        let tags = self
            .repo
            .find_tags(&digest)
            .map(Result::unwrap)
            .map(|s| s.to_string())
            .collect()
            .await;
        let data = proto::FindTagsResponse { tags };
        Ok(Response::new(data))
    }

    async fn iter_tag_specs(
        &self,
        _request: tonic::Request<proto::IterTagSpecsRequest>,
    ) -> Result<tonic::Response<proto::IterTagSpecsResponse>, tonic::Status> {
        let tag_specs = self
            .repo
            .iter_tags()
            .map(Result::unwrap)
            .map(|(s, _)| s.to_string())
            .collect()
            .await;
        let data = proto::IterTagSpecsResponse { tag_specs };
        Ok(Response::new(data))
    }

    async fn read_tag(
        &self,
        request: tonic::Request<proto::ReadTagRequest>,
    ) -> Result<tonic::Response<proto::ReadTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag_spec = request.tag_spec.parse().unwrap();
        let tags = self
            .repo
            .read_tag(&tag_spec)
            .await
            .unwrap()
            .map(Result::unwrap)
            .map(|t| (&t).into())
            .collect()
            .await;
        let data = proto::ReadTagResponse { tags };
        Ok(Response::new(data))
    }

    async fn push_raw_tag(
        &self,
        request: tonic::Request<proto::PushRawTagRequest>,
    ) -> Result<tonic::Response<proto::PushRawTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag = request.tag.try_into().unwrap();
        self.repo.push_raw_tag(&tag).await.unwrap();
        let data = proto::PushRawTagResponse {};
        Ok(Response::new(data))
    }

    async fn remove_tag_stream(
        &self,
        request: tonic::Request<proto::RemoveTagStreamRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagStreamResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag_spec = request.tag_spec.parse().unwrap();
        self.repo
            .remove_tag_stream(&tag_spec)
            .await
            .unwrap();
        let data = proto::RemoveTagStreamResponse {};
        Ok(Response::new(data))
    }

    async fn remove_tag(
        &self,
        request: tonic::Request<proto::RemoveTagRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag = request.tag.try_into().unwrap();
        self.repo.remove_tag(&tag).await.unwrap();
        let data = proto::RemoveTagResponse {};
        Ok(Response::new(data))
    }
}

impl TagService {
    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> TagServiceServer<Self> {
        TagServiceServer::new(Self { repo })
    }
}
