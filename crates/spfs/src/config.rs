// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use storage::{FromConfig, FromUrl};
use tokio_stream::StreamExt;

use crate::{runtime, storage, Result};

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

const DEFAULT_USER_STORAGE: &str = "spfs";
const FALLBACK_STORAGE_ROOT: &str = "/tmp/spfs";

fn default_fuse_worker_threads() -> usize {
    let num_cpu = num_cpus::get();
    // typically fuse does not need a huge number of threads
    // and we want to allow for many spfs fuse instances running
    // on a host without quickly consuming the thread limit
    std::cmp::min(num_cpu, 8)
}

const fn default_fuse_max_blocking_threads() -> usize {
    // the current default for tokio as of writing
    512
}

fn default_monitor_worker_threads() -> usize {
    let num_cpu = num_cpus::get();
    // typically fuse does not need a huge number of threads
    // and we want to allow for many spfs fuse instances running
    // on a host without quickly consuming the thread limit
    std::cmp::min(num_cpu, 2)
}

const fn default_monitor_max_blocking_threads() -> usize {
    // the monitor runs in the background and does
    // minimal work over time. It does not need a lot of
    // blocking threads as it will work through things in time
    2
}

static CONFIG: OnceCell<RwLock<Arc<Config>>> = OnceCell::new();

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
pub struct Storage {
    pub root: PathBuf,
    /// If true, when rendering payloads, allow hard links even if the payload
    /// is owned by a different user than the current user. Only applies to
    /// payloads readable by "other".
    pub allow_payload_sharing_between_users: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            root: dirs::data_local_dir()
                .map(|data| data.join(DEFAULT_USER_STORAGE))
                .unwrap_or_else(|| PathBuf::from(FALLBACK_STORAGE_ROOT)),
            allow_payload_sharing_between_users: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Remote {
    Address(RemoteAddress),
    Config(RemoteConfig),
}

impl ToAddress for Remote {
    fn to_address(&self) -> Result<url::Url> {
        match self {
            Self::Address(addr) => Ok(addr.address.clone()),
            Self::Config(conf) => conf.to_address(),
        }
    }
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
    Proxy(storage::proxy::Config),
}

impl ToAddress for RemoteConfig {
    fn to_address(&self) -> Result<url::Url> {
        match self {
            Self::Fs(c) => c.to_address(),
            Self::Grpc(c) => c.to_address(),
            Self::Tar(c) => c.to_address(),
            Self::Proxy(c) => c.to_address(),
        }
    }
}

impl RemoteConfig {
    /// Parse a complete repository connection config from a url
    pub async fn from_address(url: url::Url) -> Result<Self> {
        Ok(match url.scheme() {
            "tar" => Self::Tar(storage::tar::Config::from_url(&url).await?),
            "file" | "" => Self::Fs(storage::fs::Config::from_url(&url).await?),
            "http2" | "grpc" => Self::Grpc(storage::rpc::Config::from_url(&url).await?),
            "proxy" => Self::Proxy(storage::proxy::Config::from_url(&url).await?),
            scheme => return Err(format!("Unsupported repository scheme: '{scheme}'").into()),
        })
    }

    /// Parse a complete repository connection from an address string
    pub async fn from_str<S: AsRef<str>>(address: S) -> Result<Self> {
        let url = match url::Url::parse(address.as_ref()) {
            Ok(url) => url,
            Err(err) => return Err(err.into()),
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
            Self::Proxy(config) => storage::proxy::ProxyRepository::from_config(config)
                .await?
                .into(),
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Filesystem {
    /// The default mount backend to be used for new runtimes.
    pub backend: crate::runtime::MountBackend,
    /// The named remotes that can be used by the runtime
    /// file systems to find object data (if possible)
    ///
    /// This option is typically only relevant for virtual file
    /// systems that can perform read-through lookups, such as FUSE.
    #[serde(default = "Filesystem::default_secondary_repositories")]
    pub secondary_repositories: Vec<String>,
}

impl Filesystem {
    /// The default set of secondary repositories to be used by
    /// the runtime filesystem
    pub fn default_secondary_repositories() -> Vec<String> {
        vec![String::from("origin")]
    }
}

/// Configuration options for the fuse filesystem process
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Fuse {
    #[serde(default = "default_fuse_worker_threads")]
    pub worker_threads: usize,
    #[serde(default = "default_fuse_max_blocking_threads")]
    pub max_blocking_threads: usize,
}

impl Default for Fuse {
    fn default() -> Self {
        Self {
            worker_threads: default_fuse_worker_threads(),
            max_blocking_threads: default_fuse_max_blocking_threads(),
        }
    }
}

/// Configuration options for the monitor process
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Monitor {
    #[serde(default = "default_monitor_worker_threads")]
    pub worker_threads: usize,
    #[serde(default = "default_monitor_max_blocking_threads")]
    pub max_blocking_threads: usize,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            worker_threads: default_monitor_worker_threads(),
            max_blocking_threads: default_monitor_max_blocking_threads(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub user: User,
    pub storage: Storage,
    pub filesystem: Filesystem,
    pub remote: std::collections::HashMap<String, Remote>,
    pub fuse: Fuse,
    pub monitor: Monitor,
}

impl Config {
    /// Get the current loaded config, loading it if needed
    pub fn current() -> Result<Arc<Self>> {
        get_config()
    }

    /// Load the config from disk, even if it's already been loaded before
    pub fn load() -> Result<Self> {
        load_config()
    }

    /// Make this config the current global one
    pub fn make_current(self) -> Result<Arc<Self>> {
        // Note we don't know if we won the race to set the value here,
        // so we still need to try to update it.
        let config = CONFIG.get_or_try_init(|| -> Result<RwLock<Arc<Config>>> {
            Ok(RwLock::new(Arc::new(self.clone())))
        })?;

        let mut lock = config.write().map_err(|err| {
            crate::Error::String(format!(
                "Cannot update config; lock has been poisoned: {err:?}"
            ))
        })?;
        *Arc::make_mut(&mut lock) = self;
        Ok(Arc::clone(&lock))
    }

    /// Load the given string as a config
    #[deprecated(
        since = "0.32.0",
        note = "use the appropriate serde crate to deserialize a Config directly"
    )]
    pub fn load_string<S: AsRef<str>>(conf: S) -> Result<Self> {
        let mut s = config::Config::default();
        #[allow(deprecated)]
        s.merge(config::File::from_str(
            conf.as_ref(),
            config::FileFormat::Ini,
        ))?;
        Ok(Config::deserialize(s)?)
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
    pub async fn get_local_repository(&self) -> Result<storage::fs::FSRepository> {
        // Possibly use a different path for the local repository, depending
        // on enabled features.
        #[allow(unused_mut)]
        let mut use_ci_isolated_storage_path: Option<PathBuf> = None;

        #[cfg(feature = "gitlab-ci-local-repo-isolation")]
        if let Ok(id) = std::env::var("CI_PIPELINE_ID") {
            use_ci_isolated_storage_path =
                Some(self.storage.root.join("ci").join(format!("pipeline_{id}")));
        }

        storage::fs::FSRepository::create(
            use_ci_isolated_storage_path
                .as_ref()
                .unwrap_or(&self.storage.root),
        )
        .await
    }

    /// Get the local repository handle as configured.
    pub async fn get_local_repository_handle(&self) -> Result<storage::RepositoryHandle> {
        Ok(self.get_local_repository().await?.into())
    }

    /// Get a remote repository by name, or the local repository.
    ///
    /// If `name` is defined, attempt to open the named remote
    /// repository; otherwise open the local repository.
    pub async fn get_remote_repository_or_local<S>(
        &self,
        name: Option<S>,
    ) -> Result<storage::RepositoryHandle>
    where
        S: AsRef<str>,
    {
        match name {
            Some(name) => self.get_remote(name).await,
            None => Ok(self.get_local_repository().await?.into()),
        }
    }

    /// Get the local runtime storage, as configured.
    pub async fn get_runtime_storage(&self) -> Result<runtime::Storage> {
        Ok(runtime::Storage::new(storage::RepositoryHandle::from(
            self.get_local_repository().await?,
        )))
    }

    /// Get a remote repository by name.
    pub async fn get_remote<S: AsRef<str>>(
        &self,
        remote_name: S,
    ) -> Result<storage::RepositoryHandle> {
        match self.remote.get(remote_name.as_ref()) {
            Some(Remote::Address(remote)) => {
                let config = RemoteConfig::from_address(remote.address.clone()).await?;
                tracing::debug!(?config, "opening repository");
                config.open().await
            }
            Some(Remote::Config(config)) => {
                tracing::debug!(?config, "opening repository");
                config.open().await
            }
            None => Err(crate::Error::UnknownRemoteName(
                remote_name.as_ref().to_owned(),
            )),
        }
        .map_err(|err| match err {
            err @ crate::Error::FailedToOpenRepository { .. } => err,
            err => crate::Error::FailedToOpenRepository {
                repository: remote_name.as_ref().to_owned(),
                source: Box::new(err),
            },
        })
    }

    pub fn get_secondary_runtime_repositories(&self) -> Vec<url::Url> {
        let mut addrs = Vec::new();
        for name in self.filesystem.secondary_repositories.iter() {
            let Some(remote) = self.remote.get(name) else {
                tracing::warn!("Unknown secondary runtime repository: {name}");
                continue;
            };
            let Ok(addr) = remote.to_address() else {
                tracing::warn!("Cannot construct a valid address for remote: {name}");
                continue;
            };
            addrs.push(addr);
        }
        addrs
    }
}

/// An item that can be converted into a web address
pub trait ToAddress {
    fn to_address(&self) -> Result<url::Url>;
}

/// Get the current spfs config, fetching it from disk if needed.
pub fn get_config() -> Result<Arc<Config>> {
    let config = CONFIG.get_or_try_init(|| -> Result<RwLock<Arc<Config>>> {
        Ok(RwLock::new(Arc::new(load_config()?)))
    })?;
    let lock = config.read().map_err(|err| {
        crate::Error::String(format!(
            "Cannot load config, lock has been poisoned: {err:?}"
        ))
    })?;
    Ok(Arc::clone(&*lock))
}

/// Load the spfs configuration from disk, even if it's already been loaded.
///
/// This includes the default, user and system configurations, if they exist.
pub fn load_config() -> Result<Config> {
    use config::FileFormat::Ini;
    use config::{Config as RawConfig, Environment, File};

    const USER_CONFIG_BASE: &str = "spfs/spfs";
    let user_config = dirs::config_local_dir()
        .map(|config| config.join(USER_CONFIG_BASE))
        .ok_or_else(|| {
            crate::Error::String(
                "User config area could not be found, this platform may not be supported".into(),
            )
        })?;

    let config = RawConfig::builder()
        // for backwards compatibility we also support .conf as an ini extension
        .add_source(File::new("/etc/spfs.conf", Ini).required(false))
        // the system config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name("/etc/spfs").required(false))
        // for backwards compatibility we also support .conf as an ini extension
        .add_source(File::new(&format!("{}.conf", user_config.display()), Ini).required(false))
        // the user config can also be in any support format: toml, yaml, json, ini, etc
        .add_source(File::with_name(&format!("{}", user_config.display())).required(false))
        .add_source(Environment::with_prefix("SPFS").separator("_"))
        .build()?;

    Ok(Config::deserialize(config)?)
}

/// Open the repository at the given url address
pub async fn open_repository<S: AsRef<str>>(
    address: S,
) -> crate::Result<storage::RepositoryHandle> {
    match RemoteConfig::from_str(address.as_ref()).await?.open().await {
        Ok(repo) => Ok(repo),
        err @ Err(crate::Error::FailedToOpenRepository { .. }) => err,
        Err(err) => Err(crate::Error::FailedToOpenRepository {
            repository: address.as_ref().to_owned(),
            source: Box::new(err),
        }),
    }
}

/// Open a repository either by address or by configured name.
///
/// If `specifier` is `None`, return the configured local repository.
///
/// This function will try to interpret the given repository specifier
/// as either a url or configured remote name. It is recommended to use
/// an alternative function when the type of the specifier is known.
///
/// When the repository specifier is expected to be a configured
/// repository, use `config::get_remote_repository_or_local` instead.
///
/// When the repository specifier is a url, use `open_repository` instead.
pub async fn open_repository_from_string<S: AsRef<str>>(
    config: &Config,
    specifier: Option<S>,
) -> crate::Result<storage::RepositoryHandle> {
    // Discovering that the given string is not a configured remote
    // name is relatively cheap, so attempt to open a remote that
    // way first.
    let rh = config.get_remote_repository_or_local(specifier).await;

    if let Err(crate::Error::FailedToOpenRepository { source, .. }) = &rh {
        if let Some(crate::Error::UnknownRemoteName(specifier)) =
            source.downcast_ref::<crate::Error>()
        {
            // In the event that provided specifier was not a recognized name,
            // attempt to use it as an address instead.
            let rh_as_address = open_repository(specifier).await;

            // This might fail because the specifier was not a valid url.
            if let Err(crate::Error::InvalidRemoteUrl(_)) = rh_as_address {
                // If the specifier does not contain a '/' then it is more
                // likely a bare name like "foo" and not intended to be
                // treated as path on disk.
                if !specifier.contains('/') {
                    // Return the original error so the user sees something like
                    // "foo" is an unknown remote, rather than an error about
                    // parsing urls.
                    return rh;
                }

                // As a convenience, try turning the specifier into a valid file url.
                let address = format!("file:{specifier}");
                // User should see the error from this however this plays out.
                return open_repository(address).await;
            }

            // Other errors apart from parsing the url should be shown to the user.
            return rh_as_address;
        }
    }

    // No fallbacks worked so return the original result.
    rh
}
