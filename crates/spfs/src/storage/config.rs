// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use async_trait::async_trait;

use crate::Result;

/// A type that can be constructed using a url
#[async_trait]
pub trait FromUrl: Sized {
    async fn from_url(url: &url::Url) -> Result<Self>;
}

/// A type that can be constructed from some
/// existing configuration object
#[async_trait]
pub trait FromConfig: Sized {
    type Config: FromUrl + Send;

    async fn from_config(config: Self::Config) -> Result<Self>;
}

#[async_trait]
impl<T> FromUrl for T
where
    T: FromConfig + Send + Sync + Sized,
{
    async fn from_url(url: &url::Url) -> Result<Self> {
        let config = T::Config::from_url(url).await?;
        Self::from_config(config).await
    }
}
