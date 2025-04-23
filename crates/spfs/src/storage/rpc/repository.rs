// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use storage::FromUrl;

use crate::config::ToAddress;
use crate::proto::database_service_client::DatabaseServiceClient;
use crate::proto::payload_service_client::PayloadServiceClient;
use crate::proto::repository_client::RepositoryClient;
use crate::proto::tag_service_client::TagServiceClient;
use crate::storage::{OpenRepositoryError, OpenRepositoryResult, TagNamespace, TagNamespaceBuf};
use crate::{Result, proto, storage};

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

    /// The global timeout for all requests made in this client
    ///
    /// Default is no timeout
    pub timeout_ms: Option<u64>,

    /// Maximum message size that the client will accept from the server
    ///
    /// Default is 4 Mb
    pub max_decode_message_size_bytes: Option<usize>,

    /// Maximum message size that the client will sent to the server
    ///
    /// Default is no limit
    pub max_encode_message_size_bytes: Option<usize>,

    /// optional tag namespace to use when querying tags
    pub tag_namespace: Option<TagNamespaceBuf>,
}

#[async_trait::async_trait]
impl FromUrl for Config {
    async fn from_url(url: &url::Url) -> crate::storage::OpenRepositoryResult<Self> {
        let mut address = url.clone();
        let params = if let Some(qs) = address.query() {
            serde_qs::from_str(qs)
                .map_err(|source| crate::storage::OpenRepositoryError::invalid_query(url, source))?
        } else {
            Params::default()
        };
        address.set_query(None);
        Ok(Self { address, params })
    }
}

impl ToAddress for Config {
    fn to_address(&self) -> Result<url::Url> {
        let query = serde_qs::to_string(&self.params).map_err(|err| {
            crate::Error::String(format!(
                "Grpc repo parameters do not create a valid url: {err:?}"
            ))
        })?;
        let mut address = self.address.clone();
        address.set_query(Some(&query));
        Ok(address)
    }
}

#[derive(Clone, Debug)]
pub struct RpcRepository {
    address: url::Url,
    pub(super) repo_client: RepositoryClient<tonic::transport::Channel>,
    pub(super) tag_client: TagServiceClient<tonic::transport::Channel>,
    pub(super) db_client: DatabaseServiceClient<tonic::transport::Channel>,
    pub(super) payload_client: PayloadServiceClient<tonic::transport::Channel>,
    pub(super) http_client: hyper::client::conn::http1::Builder,
    /// the namespace to use for tag resolution. If set, then this is treated
    /// as "chroot" of the real tag root.
    tag_namespace: Option<TagNamespaceBuf>,
}

#[async_trait::async_trait]
impl storage::FromConfig for RpcRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> OpenRepositoryResult<Self> {
        Self::new(config).await
    }
}

impl RpcRepository {
    #[deprecated(
        since = "0.32.0",
        note = "instead, use the spfs::storage::FromUrl trait: RpcRepository::from_url(address)"
    )]
    pub async fn connect(address: url::Url) -> OpenRepositoryResult<Self> {
        Self::from_url(&address).await
    }

    /// Create a new rpc repository client for the given configuration
    pub async fn new(config: Config) -> OpenRepositoryResult<Self> {
        let mut endpoint = tonic::transport::Endpoint::from_shared(config.address.to_string())
            .map_err(|source| OpenRepositoryError::InvalidTransportAddress {
                address: config.address.to_string(),
                source,
            })?;
        if let Some(ms) = config.params.timeout_ms {
            endpoint = endpoint.timeout(std::time::Duration::from_millis(ms));
        }
        let channel = match config.params.lazy {
            true => endpoint.connect_lazy(),
            false => endpoint.connect().await?,
        };
        let mut repo_client = RepositoryClient::new(channel.clone());
        let mut tag_client = TagServiceClient::new(channel.clone());
        let mut db_client = DatabaseServiceClient::new(channel.clone());
        let mut payload_client = PayloadServiceClient::new(channel);
        if let Some(max) = config.params.max_decode_message_size_bytes {
            repo_client = repo_client.max_decoding_message_size(max);
            tag_client = tag_client.max_decoding_message_size(max);
            db_client = db_client.max_decoding_message_size(max);
            payload_client = payload_client.max_decoding_message_size(max);
        }
        if let Some(max) = config.params.max_encode_message_size_bytes {
            repo_client = repo_client.max_encoding_message_size(max);
            tag_client = tag_client.max_encoding_message_size(max);
            db_client = db_client.max_encoding_message_size(max);
            payload_client = payload_client.max_encoding_message_size(max);
        }
        Ok(Self {
            address: config.to_address().expect("an internally valid config"),
            repo_client,
            tag_client,
            db_client,
            payload_client,
            http_client: hyper::client::conn::http1::Builder::new(),
            tag_namespace: config.params.tag_namespace,
        })
    }

    /// The round-trip time taken to ping this repository over grpc, if successful
    pub async fn ping(&self) -> Result<std::time::Duration> {
        let start = std::time::Instant::now();
        self.repo_client.clone().ping(proto::PingRequest {}).await?;
        Ok(start.elapsed())
    }

    /// The namespace to use for tag resolution.
    pub fn tag_namespace(&self) -> Option<&TagNamespace> {
        self.tag_namespace.as_deref()
    }

    /// Set the namespace to use for tag resolution.
    ///
    /// Returns the previous namespace, if any.
    pub fn set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Option<TagNamespaceBuf> {
        std::mem::replace(&mut self.tag_namespace, tag_namespace)
    }
}

impl storage::Address for RpcRepository {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Borrowed(&self.address)
    }
}
