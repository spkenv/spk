// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::proto::{
    database_service_client::DatabaseServiceClient, payload_service_client::PayloadServiceClient,
    repository_client::RepositoryClient, tag_service_client::TagServiceClient,
};
use crate::{proto, storage, Error, Result};

/// Configures an rpc repository connection
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub address: url::Url,
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        Ok(Self {
            address: url.clone(),
        })
    }
}

#[derive(Debug)]
pub struct RpcRepository {
    address: url::Url,
    pub(super) repo_client: RepositoryClient<tonic::transport::Channel>,
    pub(super) tag_client: TagServiceClient<tonic::transport::Channel>,
    pub(super) db_client: DatabaseServiceClient<tonic::transport::Channel>,
    pub(super) payload_client: PayloadServiceClient<tonic::transport::Channel>,
}

#[async_trait::async_trait]
impl storage::FromConfig for RpcRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> Result<Self> {
        Self::connect(config.address).await
    }
}

impl RpcRepository {
    pub async fn connect(address: url::Url) -> Result<Self> {
        let endpoint =
            tonic::transport::Endpoint::from_shared(address.to_string()).map_err(|err| {
                Error::String(format!("invalid address for rpc repository: {:?}", err))
            })?;
        let repo_client = RepositoryClient::connect(endpoint.clone())
            .await
            .map_err(|err| {
                Error::String(format!("failed to connect to rpc repository: {:?}", err))
            })?;
        let tag_client = TagServiceClient::connect(endpoint.clone())
            .await
            .map_err(|err| {
                Error::String(format!("failed to connect to rpc repository: {:?}", err))
            })?;
        let db_client = DatabaseServiceClient::connect(endpoint.clone())
            .await
            .map_err(|err| {
                Error::String(format!("failed to connect to rpc repository: {:?}", err))
            })?;
        let payload_client = PayloadServiceClient::connect(endpoint)
            .await
            .map_err(|err| {
                Error::String(format!("failed to connect to rpc repository: {:?}", err))
            })?;
        Ok(Self {
            address,
            repo_client,
            tag_client,
            db_client,
            payload_client,
        })
    }

    /// The round-trip time taken to ping this repository over grpc, if successful
    pub async fn ping(&self) -> Result<std::time::Duration> {
        let start = std::time::Instant::now();
        self.repo_client.clone().ping(proto::PingRequest {}).await?;
        Ok(start.elapsed())
    }
}

impl storage::Repository for RpcRepository {
    fn address(&self) -> url::Url {
        self.address.clone()
    }
}
