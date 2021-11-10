// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::proto::repository_client::RepositoryClient;
use crate::{storage, Error, Result};

#[derive(Debug)]
pub struct RpcRepository {
    address: url::Url,
    pub(super) client: RepositoryClient<tonic::transport::Channel>,
}

impl RpcRepository {
    pub async fn connect(address: url::Url) -> Result<Self> {
        let endpoint =
            tonic::transport::Endpoint::from_shared(address.to_string()).map_err(|err| {
                Error::String(format!("invalid address for rpc repository: {:?}", err))
            })?;
        let client = RepositoryClient::connect(endpoint).await.map_err(|err| {
            Error::String(format!("failed to connect to rpc repository: {:?}", err))
        })?;
        Ok(Self { address, client })
    }
}

impl storage::Repository for RpcRepository {
    fn address(&self) -> url::Url {
        self.address.clone()
    }
}
