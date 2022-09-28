// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use storage::FromUrl;

use crate::proto::database_service_client::DatabaseServiceClient;
use crate::proto::payload_service_client::PayloadServiceClient;
use crate::proto::repository_client::RepositoryClient;
use crate::proto::tag_service_client::TagServiceClient;
use crate::{proto, storage, Error, Result};

/// Configures an rpc repository connection
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub address: url::Url,
    #[serde(flatten)]
    pub params: Params,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Default)]
pub struct Params {
    /// if true, don't actually attempt to connect until first use
    #[serde(default)]
    pub lazy: bool,
}

#[async_trait::async_trait]
impl FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        let mut address = url.clone();
        let params = if let Some(qs) = address.query() {
            serde_qs::from_str(qs).map_err(|err| {
                crate::Error::String(format!("Invalid grpc repo parameters: {:?}", err))
            })?
        } else {
            Params::default()
        };
        address.set_query(None);
        Ok(Self { address, params })
    }
}

impl Config {
    pub fn to_address(&self) -> Result<url::Url> {
        let query = serde_qs::to_string(&self.params).map_err(|err| {
            crate::Error::String(format!(
                "Grpc repo parameters do not create a valid url: {:?}",
                err
            ))
        })?;
        let mut address = self.address.clone();
        address.set_query(Some(&query));
        Ok(address)
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
        Self::new(config).await
    }
}

impl RpcRepository {
    #[deprecated(
        since = "0.32.0",
        note = "instead, use the spfs::storage::FromUrl trait: RpcRepository::from_url(address)"
    )]
    pub async fn connect(address: url::Url) -> Result<Self> {
        Self::from_url(&address).await
    }

    /// Create a new rpc repository client for the given configuration
    pub async fn new(config: Config) -> Result<Self> {
        let endpoint = tonic::transport::Endpoint::from_shared(config.address.to_string())
            .map_err(|err| {
                Error::String(format!("invalid address for rpc repository: {:?}", err))
            })?;
        let channel = match config.params.lazy {
            true => endpoint.connect_lazy(),
            false => endpoint.connect().await.map_err(|err| {
                Error::String(format!("failed to connect to rpc repository: {:?}", err))
            })?,
        };
        let repo_client = RepositoryClient::new(channel.clone());
        let tag_client = TagServiceClient::new(channel.clone());
        let db_client = DatabaseServiceClient::new(channel.clone());
        let payload_client = PayloadServiceClient::new(channel);
        Ok(Self {
            address: config.to_address()?,
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
