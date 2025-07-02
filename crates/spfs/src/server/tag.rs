// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::convert::TryInto;
use std::sync::Arc;

use futures::TryStreamExt;
use relative_path::RelativePath;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::prelude::*;
use crate::proto::tag_service_server::TagServiceServer;
use crate::proto::{self, RpcResult, convert_digest};
use crate::storage::{self, TagNamespace};

fn string_to_namespace(namespace: &String) -> Option<&TagNamespace> {
    if namespace.is_empty() {
        None
    } else {
        Some(
            TagNamespace::new(RelativePath::new(namespace))
                .expect("namespace was valid before being passed over rpc as a string"),
        )
    }
}

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
        tracing::trace!("receive request");
        let request = request.into_inner();
        let path = relative_path::RelativePath::new(&request.path);
        let entries: crate::Result<Vec<_>> = {
            self.repo
                .ls_tags_in_namespace(string_to_namespace(&request.namespace), path)
                .collect()
                .await
        };
        let entries = proto::handle_error!(entries);
        let entries = entries.iter().map(|e| e.into()).collect();

        let data = proto::LsTagsResponse::ok(proto::ls_tags_response::EntryList { entries });
        Ok(Response::new(data))
    }

    async fn resolve_tag(
        &self,
        request: tonic::Request<proto::ResolveTagRequest>,
    ) -> Result<tonic::Response<proto::ResolveTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag_spec = proto::handle_error!(request.tag_spec.parse());
        let tag = proto::handle_error!(
            self.repo
                .resolve_tag_in_namespace(string_to_namespace(&request.namespace), &tag_spec)
                .await
        );
        let data = proto::ResolveTagResponse::ok((&tag).into());
        Ok(Response::new(data))
    }

    async fn find_tags(
        &self,
        request: tonic::Request<proto::FindTagsRequest>,
    ) -> Result<tonic::Response<proto::FindTagsResponse>, tonic::Status> {
        let request = request.into_inner();
        let digest = proto::handle_error!(convert_digest(request.digest));
        let mut results = self
            .repo
            .find_tags_in_namespace(string_to_namespace(&request.namespace), &digest);
        let mut tags = Vec::new();
        while let Some(item) = results.next().await {
            let item = proto::handle_error!(item);
            tags.push(item.to_string());
        }
        let data = proto::FindTagsResponse::ok(proto::find_tags_response::TagList { tags });
        Ok(Response::new(data))
    }

    async fn iter_tag_specs(
        &self,
        request: tonic::Request<proto::IterTagSpecsRequest>,
    ) -> Result<tonic::Response<proto::IterTagSpecsResponse>, tonic::Status> {
        let request = request.into_inner();
        let mut streams = self
            .repo
            .iter_tags_in_namespace(string_to_namespace(&request.namespace));
        let mut tag_specs = Vec::new();
        while let Some(item) = streams.next().await {
            let item = proto::handle_error!(item);
            tag_specs.push(item.0.to_string());
        }
        let data = proto::IterTagSpecsResponse::ok(proto::iter_tag_specs_response::TagSpecList {
            tag_specs,
        });
        Ok(Response::new(data))
    }

    async fn read_tag(
        &self,
        request: tonic::Request<proto::ReadTagRequest>,
    ) -> Result<tonic::Response<proto::ReadTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag_spec = proto::handle_error!(request.tag_spec.parse());
        let stream = proto::handle_error!(
            self.repo
                .read_tag_in_namespace(string_to_namespace(&request.namespace), &tag_spec)
                .await
        );

        let tags: crate::Result<Vec<_>> = stream.map_ok(|t| (&t).into()).collect().await;
        let tags = proto::handle_error!(tags);
        let data = proto::ReadTagResponse::ok(proto::read_tag_response::TagList { tags });
        Ok(Response::new(data))
    }

    async fn insert_tag(
        &self,
        request: tonic::Request<proto::InsertTagRequest>,
    ) -> Result<tonic::Response<proto::InsertTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag = proto::handle_error!(request.tag.try_into());
        proto::handle_error!(
            self.repo
                .insert_tag_in_namespace(string_to_namespace(&request.namespace), &tag)
                .await
        );
        let data = proto::InsertTagResponse::ok(proto::Ok {});
        Ok(Response::new(data))
    }

    async fn remove_tag_stream(
        &self,
        request: tonic::Request<proto::RemoveTagStreamRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagStreamResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag_spec = proto::handle_error!(request.tag_spec.parse());
        proto::handle_error!(
            self.repo
                .remove_tag_stream_in_namespace(string_to_namespace(&request.namespace), &tag_spec)
                .await
        );

        let data = proto::RemoveTagStreamResponse::ok(proto::Ok {});
        Ok(Response::new(data))
    }

    async fn remove_tag(
        &self,
        request: tonic::Request<proto::RemoveTagRequest>,
    ) -> Result<tonic::Response<proto::RemoveTagResponse>, tonic::Status> {
        let request = request.into_inner();
        let tag = proto::handle_error!(request.tag.try_into());
        proto::handle_error!(
            self.repo
                .remove_tag_in_namespace(string_to_namespace(&request.namespace), &tag)
                .await
        );

        let data = proto::RemoveTagResponse::ok(proto::Ok {});
        Ok(Response::new(data))
    }
}

impl TagService {
    pub fn new(repo: Arc<storage::RepositoryHandle>) -> Self {
        Self { repo }
    }

    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> TagServiceServer<Self> {
        TagServiceServer::new(Self::new(repo))
    }
}
