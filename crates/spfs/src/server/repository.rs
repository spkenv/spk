// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use tonic::{Request, Response, Status};

use super::proto;
use crate::storage;
use proto::repository_server::RepositoryServer;

#[derive(Debug, Clone)]
pub struct Repository {
    repo: Arc<storage::RepositoryHandle>,
}

#[tonic::async_trait]
impl proto::repository_server::Repository for Repository {
    async fn ping(
        &self,
        _request: Request<proto::PingRequest>,
    ) -> std::result::Result<Response<proto::PingResponse>, Status> {
        let data = proto::PingResponse::default();
        Ok(Response::new(data))
    }
}

impl Repository {
    pub fn new_srv(repo: Arc<storage::RepositoryHandle>) -> RepositoryServer<Self> {
        RepositoryServer::new(Self { repo })
    }
}
