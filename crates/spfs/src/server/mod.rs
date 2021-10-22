// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Remote server implementations of the spfs repository
use std::sync::Arc;

use tonic::{transport::Server, Request, Response, Status};

use proto::spfs_service_server::{SpfsService, SpfsServiceServer};
pub mod proto {
    tonic::include_proto!("spfs");
}

use crate::storage;

#[derive(Debug, Clone)]
pub struct Service {
    repo: Arc<storage::RepositoryHandle>,
}

#[tonic::async_trait]
impl SpfsService for Service {
    async fn ping(
        &self,
        _request: Request<proto::PingRequest>,
    ) -> std::result::Result<Response<proto::PingResponse>, Status> {
        let data = proto::PingResponse::default();
        Ok(Response::new(data))
    }
}

pub async fn run(
    address: std::net::SocketAddr,
    repo: storage::RepositoryHandle,
) -> crate::Result<()> {
    let service = Service {
        repo: Arc::new(repo),
    };

    let builder = Server::builder().add_service(SpfsServiceServer::new(service));
    tracing::info!("server is listening on: {}", address);

    let server = builder.serve(address);
    server.await.unwrap();
    Ok(())
}
