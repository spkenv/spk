// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use config::{Config as ConfigBase, Environment, File};
use storage::{FromConfig, FromUrl};
use tokio_stream::StreamExt;

use crate::{runtime, storage, Result};
use std::path::PathBuf;

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

static DEFAULT_STORAGE_ROOT: &str = "~/.local/share/spfs";
static FALLBACK_STORAGE_ROOT: &str = "/tmp/spfs";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct User {
    pub name: String,
    pub domain: String,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: whoami::username(),
            domain: whoami::hostname(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Filesystem {
    pub max_layers: usize,
    pub tmpfs_size: Option<String>,
}

impl Default for Filesystem {
    fn default() -> Self {
        Self {
            max_layers: 40,
            tmpfs_size: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Storage {
    pub root: PathBuf,
    pub runtimes: Option<PathBuf>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            root: expanduser::expanduser(DEFAULT_STORAGE_ROOT)
                .unwrap_or_else(|_| PathBuf::from(FALLBACK_STORAGE_ROOT)),
            runtimes: None,
        }
    }
}

impl Storage {
    /// Return the path to the local runtime storage.
    pub fn runtime_root(&self) -> PathBuf {
        match &self.runtimes {
            None => self.root.join("runtimes"),
            Some(root) => root.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Remote {
    Address(RemoteAddress),
    Config(RemoteConfig),
}

impl<'de> serde::de::Deserialize<'de> for Remote {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde_json::{Map, Value};
        let data = Map::deserialize(deserializer)?;
        if data.contains_key(&String::from("scheme")) {
            Ok(Self::Config(
                RemoteConfig::deserialize(Value::Object(data)).map_err(serde::de::Error::custom)?,
            ))
        } else {
            Ok(Self::Address(
                RemoteAddress::deserialize(Value::Object(data))
                    .map_err(serde::de::Error::custom)?,
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RemoteAddress {
    pub address: url::Url,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "scheme", rename_all = "lowercase")]
pub enum RemoteConfig {
    Fs(storage::fs::Config),
    Grpc(storage::rpc::Config),
    Tar(storage::tar::Config),
}

impl RemoteConfig {
    /// Parse a complete repository connection config from a url
    pub async fn from_address(url: url::Url) -> Result<Self> {
        Ok(match url.scheme() {
            "tar" => Self::Tar(storage::tar::Config::from_url(&url).await?),
            "file" | "" => Self::Fs(storage::fs::Config::from_url(&url).await?),
            "http2" | "grpc" => Self::Grpc(storage::rpc::Config::from_url(&url).await?),
            scheme => return Err(format!("Unsupported repository scheme: '{scheme}'").into()),
        })
    }

    /// Parse a complete repository connection from an address string
    pub async fn from_str<S: AsRef<str>>(address: S) -> Result<Self> {
        let url = match url::Url::parse(address.as_ref()) {
            Ok(url) => url,
            Err(err) => return Err(format!("invalid repository url: {:?}", err).into()),
        };

        Self::from_address(url).await
    }

    /// Open a handle to a repository using this configuration
    pub async fn open(&self) -> Result<storage::RepositoryHandle> {
        Ok(match self.clone() {
            Self::Fs(config) => storage::fs::FSRepository::from_config(config).await?.into(),
            Self::Tar(config) => storage::tar::TarRepository::from_config(config)
                .await?
                .into(),
            Self::Grpc(config) => storage::rpc::RpcRepository::from_config(config)
                .await?
                .into(),
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub user: User,
    pub storage: Storage,
    pub filesystem: Filesystem,
    pub remote: std::collections::HashMap<String, Remote>,
}

impl Config {
    pub fn load_string<S: AsRef<str>>(conf: S) -> Result<Self> {
        let mut s = ConfigBase::new();
        s.merge(config::File::from_str(
            conf.as_ref(),
            config::FileFormat::Ini,
        ))?;
        Ok(s.try_into()?)
    }

    /// List the names of all configured remote repositories.
    pub fn list_remote_names(&self) -> Vec<String> {
        self.remote.keys().map(|s| s.to_string()).collect()
    }

    /// Open a connection to all remote repositories
    pub async fn list_remotes(&self) -> Result<Vec<storage::RepositoryHandle>> {
        let futures: futures::stream::FuturesUnordered<_> =
            self.remote.keys().map(|s| self.get_remote(s)).collect();
        futures.collect().await
    }

    /// Get the local repository instance as configured.
    pub async fn get_repository(&self) -> Result<storage::fs::FSRepository> {
        storage::fs::FSRepository::create(&self.storage.root).await
    }

    /// Get the local runtime storage, as configured.
    pub fn get_runtime_storage(&self) -> Result<runtime::Storage> {
        runtime::Storage::new(self.storage.runtime_root())
    }

    /// Get a remote repostory by name or address.
    pub async fn get_remote<S: AsRef<str>>(
        &self,
        name_or_address: S,
    ) -> Result<storage::RepositoryHandle> {
        match self.remote.get(name_or_address.as_ref()) {
            Some(Remote::Address(remote)) => {
                let config = RemoteConfig::from_address(remote.address.clone()).await?;
                tracing::debug!(?config, "opening repository");
                config.open().await
            }
            Some(Remote::Config(config)) => {
                tracing::debug!(?config, "opening repository");
                config.open().await
            }
            None => {
                let addr = match url::Url::parse(name_or_address.as_ref()) {
                    Ok(addr) => addr,
                    Err(_) => {
                        url::Url::parse(format!("file:{}", name_or_address.as_ref()).as_str())?
                    }
                };
                let config = RemoteConfig::from_address(addr).await?;
                tracing::debug!(?config, "opening repository");
                config.open().await
            }
        }
    }
}

/// Load the spfs configuration from disk.
///
/// This includes the default, user and system configurations, if they exist.
pub fn load_config() -> Result<Config> {
    let user_config = expanduser::expanduser("~/.config/spfs/spfs.conf")?;
    let system_config = PathBuf::from("/etc/spfs.conf");

    let mut s = ConfigBase::new();
    if let Some(name) = system_config.to_str() {
        s.merge(
            File::with_name(name)
                .format(config::FileFormat::Ini)
                .required(false),
        )?;
    }
    if let Some(name) = user_config.to_str() {
        s.merge(
            File::with_name(name)
                .format(config::FileFormat::Ini)
                .required(false),
        )?;
    }
    s.merge(Environment::with_prefix("SPFS").separator("_"))?;

    if let Ok(v) = s.get_str("filesystem.max.layers") {
        let _ = s.set("filesystem.max_layers", v);
    }
    if let Ok(v) = s.get_str("filesystem.tmpfs.size") {
        let _ = s.set("filesystem.tmpfs_size", v);
    }
    Ok(s.try_into()?)
}

/// Open the repository at the given url address
pub async fn open_repository<S: AsRef<str>>(
    address: S,
) -> crate::Result<storage::RepositoryHandle> {
    crate::config::RemoteConfig::from_str(address)
        .await?
        .open()
        .await
}
