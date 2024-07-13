// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use async_trait::async_trait;

use super::OpenRepositoryError;

pub type OpenRepositoryResult<T> = std::result::Result<T, OpenRepositoryError>;

/// A type that can be constructed using a url
#[async_trait]
pub trait FromUrl: Sized {
    async fn from_url(url: &url::Url) -> OpenRepositoryResult<Self>;
}

/// A type that can be constructed from some
/// existing configuration object
#[async_trait]
pub trait FromConfig: Sized {
    type Config: FromUrl + Send;

    async fn from_config(config: Self::Config) -> OpenRepositoryResult<Self>;
}

#[async_trait]
impl<T> FromUrl for T
where
    T: FromConfig + Send + Sync + Sized,
{
    async fn from_url(url: &url::Url) -> OpenRepositoryResult<Self> {
        let config = T::Config::from_url(url).await?;
        Self::from_config(config).await
    }
}
