// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use derive_builder::Builder;
use once_cell::sync::OnceCell;
use relative_path::RelativePath;
use serde::{Deserialize, Serialize};
use storage::{FromConfig, FromUrl};
use tokio_stream::StreamExt;

use crate::graph::DEFAULT_SPFS_ANNOTATION_LAYER_MAX_STRING_VALUE_SIZE;
use crate::storage::{TagNamespaceBuf, TagStorageMut};
use crate::{Error, Result, graph, runtime, storage, tracking};

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

const DEFAULT_USER_STORAGE: &str = "spfs";
const FALLBACK_STORAGE_ROOT: &str = "/tmp/spfs";
const LOCAL_STORAGE_NAME: &str = "<local storage>";

fn default_fuse_worker_threads() -> NonZeroUsize {
    let num_cpu = num_cpus::get();
    // typically fuse does not need a huge number of threads
    // and we want to allow for many spfs fuse instances running
    // on a host without quickly consuming the thread limit
    // Safety: num_cpus never returns a value of zero
    unsafe { NonZeroUsize::new_unchecked(std::cmp::min(num_cpu, 8)) }
}

const fn default_fuse_max_blocking_threads() -> NonZeroUsize {
    // the current default for tokio as of writing
    // Safety: this is a hard-coded non-zero value
    unsafe { NonZeroUsize::new_unchecked(512) }
}

fn default_monitor_worker_threads() -> NonZeroUsize {
    let num_cpu = num_cpus::get();
    // typically fuse does not need a huge number of threads
    // and we want to allow for many spfs fuse instances running
    // on a host without quickly consuming the thread limit
    // Safety: num_cpus never returns a value of zero
    unsafe { NonZeroUsize::new_unchecked(std::cmp::min(num_cpu, 2)) }
}

const fn default_monitor_max_blocking_threads() -> NonZeroUsize {
    // the monitor runs in the background and does
    // minimal work over time. It does not need a lot of
    // blocking threads as it will work through things in time
    // Safety: this is a hard-coded non-zero value
    unsafe { NonZeroUsize::new_unchecked(2) }
}

const fn default_fuse_heartbeat_interval_seconds() -> NonZeroU64 {
    // Safety: this is a hard-coded non-zero value
    unsafe { NonZeroU64::new_unchecked(60) }
}

const fn default_fuse_heartbeat_grace_period_seconds() -> NonZeroU64 {
    // Safety: this is a hard-coded non-zero value
    unsafe { NonZeroU64::new_unchecked(300) }
}

static CONFIG: OnceCell<RwLock<Arc<Config>>> = OnceCell::new();

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct User {
    pub name: String,
    pub domain: Option<String>,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: whoami::username(),
            domain: whoami::fallible::hostname().ok(),
        }
    }
}

impl std::fmt::Display for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.domain {
            Some(domain) => write!(f, "{}@{}", self.name, domain),
            None => write!(f, "{}", self.name),
        }
    }
}

/// Expand tilde ~/paths and deserialize into a PathBuf.
pub(crate) mod pathbuf_deserialize_with_tilde_expansion {
    use serde::{Deserialize, Deserializer};

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<std::path::PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.starts_with('~') {
            let expanded = shellexpand::tilde(&value);
            return Ok(std::path::PathBuf::from(expanded.as_ref()));
        }
        Ok(std::path::PathBuf::from(value))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Storage {
    #[serde(deserialize_with = "pathbuf_deserialize_with_tilde_expansion::deserialize")]
    pub root: PathBuf,
    /// If true, when rendering payloads, allow hard links even if the payload
    /// is owned by a different user than the current user. Only applies to
    /// payloads readable by "other".
    pub allow_payload_sharing_between_users: bool,
    pub tag_namespace: Option<TagNamespaceBuf>,
    /// The strategy to use when generating new objects.
    ///
    /// All available strategies are still supported for reading.
    #[serde(default)]
    pub digest_strategy: graph::object::DigestStrategy,
    /// The format to use when generating new objects.
    ///
    /// All available formats are still supported for reading.
    #[serde(default)]
    pub encoding_format: graph::object::EncodingFormat,
}

impl Storage {
    /// Get the repository address for the root storage
    pub fn address(&self) -> url::Url {
        url::Url::from_directory_path(&self.root)
            .or_else(|_err| url::Url::parse(&format!("file://{}", self.root.display())))
            .or_else(|_err| url::Url::parse(&format!("file://{}", self.root.to_string_lossy())))
            .expect("file urls should always be constructable")
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            root: dirs::data_local_dir()
                .map(|data| data.join(DEFAULT_USER_STORAGE))
                .unwrap_or_else(|| PathBuf::from(FALLBACK_STORAGE_ROOT)),
            allow_payload_sharing_between_users: false,
            tag_namespace: None,
            digest_strategy: graph::object::DigestStrategy::default(),
            encoding_format: graph::object::EncodingFormat::default(),
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

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct RemoteConfig {
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<tracking::TimeSpec>,
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_namespace: Option<TagNamespaceBuf>,
    #[serde(flatten)]
    pub inner: RepositoryConfig,
}

impl ToAddress for RemoteConfig {
    fn to_address(&self) -> Result<url::Url> {
        let Self {
            when,
            tag_namespace,
            inner,
        } = self;
        let mut inner = inner.to_address()?;
        if let Some(when) = when {
            let query = format!("when={when}");
            match inner.query() {
                None | Some("") => inner.set_query(Some(&query)),
                Some(q) => inner.set_query(Some(&format!("{q}&{query}"))),
            }
        }
        if let Some(tag_namespace) = tag_namespace {
            let query = format!("tag_namespace={}", tag_namespace.as_rel_path());
            match inner.query() {
                None | Some("") => inner.set_query(Some(&query)),
                Some(q) => inner.set_query(Some(&format!("{q}&{query}"))),
            }
        }
        Ok(inner)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "scheme", rename_all = "lowercase")]
pub enum RepositoryConfig {
    Fs(storage::fs::Config),
    Grpc(storage::rpc::Config),
    Tar(storage::tar::Config),
    Proxy(storage::proxy::Config),
}

impl ToAddress for RepositoryConfig {
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
        let mut builder = RemoteConfigBuilder::default();
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "when" => {
                    builder.when(tracking::TimeSpec::parse(v)?);
                }
                "tag_namespace" => {
                    builder.tag_namespace(TagNamespaceBuf::new(RelativePath::new(&v))?);
                }
                _ => (),
            }
        }
        let result = match url.scheme() {
            "tar" => storage::tar::Config::from_url(&url)
                .await
                .map(RepositoryConfig::Tar),
            "file" | "" => storage::fs::Config::from_url(&url)
                .await
                .map(RepositoryConfig::Fs),
            "http2" | "grpc" => storage::rpc::Config::from_url(&url)
                .await
                .map(RepositoryConfig::Grpc),
            "proxy" => storage::proxy::Config::from_url(&url)
                .await
                .map(RepositoryConfig::Proxy),
            scheme => return Err(format!("Unsupported repository scheme: '{scheme}'").into()),
        };
        builder.inner(result.map_err(|source| Error::FailedToOpenRepository {
            repository: url.to_string(),
            source,
        })?);
        Ok(builder.build().expect("No uninitialized fields"))
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
    pub async fn open(&self) -> storage::OpenRepositoryResult<storage::RepositoryHandle> {
        let Self {
            when,
            tag_namespace,
            inner,
        } = self;
        let mut handle: storage::RepositoryHandle = match inner.clone() {
            RepositoryConfig::Fs(config) => storage::fs::MaybeOpenFsRepository::from_config(config)
                .await?
                .into(),
            RepositoryConfig::Tar(config) => storage::tar::TarRepository::from_config(config)
                .await?
                .into(),
            RepositoryConfig::Grpc(config) => storage::rpc::RpcRepository::from_config(config)
                .await?
                .into(),
            RepositoryConfig::Proxy(config) => storage::proxy::ProxyRepository::from_config(config)
                .await?
                .into(),
        };
        // Set tag namespace first before pinning, because it is not possible
        // to set the tag namespace on a pinned handle.
        let handle = match tag_namespace {
            None => handle,
            Some(tag_namespace) => {
                handle
                    .try_set_tag_namespace(Some(tag_namespace.clone()))
                    .map_err(
                        |err| storage::OpenRepositoryError::FailedToSetTagNamespace {
                            tag_namespace: tag_namespace.clone(),
                            source: Box::new(err),
                        },
                    )?;
                handle
            }
        };
        let handle = match when {
            None => handle,
            Some(ts) => handle.into_pinned(ts.to_datetime_from_now()),
        };
        Ok(handle)
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

    /// The size limit for an annotation before the data is stored in
    /// a separate blob payload referenced in an annotation
    /// layer. Data values smaller than or equal to this are stored
    /// directly in the annotation layer.
    #[serde(default = "Filesystem::default_annotation_size_limit")]
    pub annotation_size_limit: usize,
}

impl Filesystem {
    /// The default set of secondary repositories to be used by
    /// the runtime filesystem
    pub fn default_secondary_repositories() -> Vec<String> {
        vec![String::from("origin")]
    }

    /// The default size limit for a piece of annotation data before it
    /// is stored in a separate blob payload from the annotation
    /// layer that contains it
    pub fn default_annotation_size_limit() -> usize {
        DEFAULT_SPFS_ANNOTATION_LAYER_MAX_STRING_VALUE_SIZE
    }
}

/// Configuration options for the fuse filesystem process
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Fuse {
    #[serde(default = "default_fuse_worker_threads")]
    pub worker_threads: NonZeroUsize,
    #[serde(default = "default_fuse_max_blocking_threads")]
    pub max_blocking_threads: NonZeroUsize,
    /// Enable a heartbeat between spfs-monitor and spfs-fuse. If spfs-monitor
    /// stops sending a heartbeat, spfs-fuse will shut down.
    pub enable_heartbeat: bool,
    /// How often to send a heartbeat, in seconds
    #[serde(default = "default_fuse_heartbeat_interval_seconds")]
    pub heartbeat_interval_seconds: NonZeroU64,
    /// How long to allow not receiving a heartbeat before shutting down, in
    /// seconds
    #[serde(default = "default_fuse_heartbeat_grace_period_seconds")]
    pub heartbeat_grace_period_seconds: NonZeroU64,
}

impl Fuse {
    /// The prefix for the heartbeat file name when heartbeats are enabled.
    /// This prefix contains a UUID so that the fuse backend can reasonably
    /// assume any attempt to access a file with this prefix is a heartbeat and
    /// does not need to process the related file I/O normally.
    pub const HEARTBEAT_FILENAME_PREFIX: &'static str =
        ".spfs-heartbeat-436cd8d6-60d1-11ef-9c93-00155dab73c6-";
}

impl Default for Fuse {
    fn default() -> Self {
        Self {
            worker_threads: default_fuse_worker_threads(),
            max_blocking_threads: default_fuse_max_blocking_threads(),
            enable_heartbeat: false,
            heartbeat_interval_seconds: default_fuse_heartbeat_interval_seconds(),
            heartbeat_grace_period_seconds: default_fuse_heartbeat_grace_period_seconds(),
        }
    }
}

/// Configuration options for the monitor process
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Monitor {
    #[serde(default = "default_monitor_worker_threads")]
    pub worker_threads: NonZeroUsize,
    #[serde(default = "default_monitor_max_blocking_threads")]
    pub max_blocking_threads: NonZeroUsize,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            worker_threads: default_monitor_worker_threads(),
            max_blocking_threads: default_monitor_max_blocking_threads(),
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Sentry {
    /// Sentry DSN
    pub dsn: String,

    /// Sentry environment
    pub environment: Option<String>,

    /// Environment variable name to use as sentry username, if set.
    ///
    /// This is useful in CI if the CI system has a variable that contains
    /// the username of the person who triggered the build.
    pub username_override_var: Option<String>,

    /// Set the email address domain used to generate email addresses for
    /// sentry events.
    pub email_domain: Option<String>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Environment {
    /// Environment variables names to preserve when creating an spfs
    /// environment.
    ///
    /// Most environment variables are preserved by default but a few are
    /// cleared for security purposes. Known values include `TMPDIR` and
    /// `LD_LIBRARY_PATH`. Any variable listed here will be propagated into a
    /// new spfs runtime by capturing their values before running spfs-enter and
    /// then setting them back to the captured values from inside the spfs
    /// runtime startup script.
    ///
    /// Any variables listed here that are not present in the environment will
    /// remain unset in the new spfs environment.
    pub variable_names_to_preserve: Vec<String>,
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
    pub sentry: Sentry,
    pub environment: Environment,
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

    /// Get the local repository instance as configured, creating it if needed.
    pub async fn get_opened_local_repository(&self) -> Result<storage::fs::OpenFsRepository> {
        // Possibly use a different path for the local repository, depending
        // on enabled features.
        #[allow(unused_mut)]
        let mut use_ci_isolated_storage_path: Option<PathBuf> = None;

        #[cfg(feature = "gitlab-ci-local-repo-isolation")]
        if let Ok(id) = std::env::var("CI_PIPELINE_ID") {
            use_ci_isolated_storage_path =
                Some(self.storage.root.join("ci").join(format!("pipeline_{id}")));
        }

        let mut local_repo = storage::fs::OpenFsRepository::create(
            use_ci_isolated_storage_path
                .as_ref()
                .unwrap_or(&self.storage.root),
        )
        .await
        .map_err(|source| Error::FailedToOpenRepository {
            repository: LOCAL_STORAGE_NAME.into(),
            source,
        })?;

        Arc::make_mut(&mut local_repo.fs_impl)
            .set_tag_namespace(self.storage.tag_namespace.clone());

        Ok(local_repo)
    }

    /// Get the local repository instance as configured, creating it if needed.
    ///
    /// The returned repo is guaranteed to be created, valid and open already. Ie
    /// the local repository is not allowed to be lazily opened.
    pub async fn get_local_repository(&self) -> Result<storage::fs::OpenFsRepository> {
        self.get_opened_local_repository().await
    }

    /// Get the local repository handle as configured,  creating it if needed.
    ///
    /// The returned repo is guaranteed to be created, valid and open already. Ie
    /// the local repository is not allowed to be lazily opened.
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
        runtime::Storage::new(storage::RepositoryHandle::from(
            self.get_local_repository().await?,
        ))
    }

    /// Get a remote repository by name, returning None if it is not configured
    pub async fn try_get_remote<S: AsRef<str>>(
        &self,
        remote_name: S,
    ) -> Result<Option<storage::RepositoryHandle>> {
        match self.get_remote(remote_name).await {
            Err(crate::Error::UnknownRemoteName(_)) => Ok(None),
            res => res.map(Some),
        }
    }

    /// Get a remote repository by name.
    pub async fn get_remote<S: AsRef<str>>(
        &self,
        remote_name: S,
    ) -> Result<storage::RepositoryHandle> {
        match self.remote.get(remote_name.as_ref()) {
            Some(Remote::Address(remote)) => {
                let config = RemoteConfig::from_address(remote.address.clone()).await?;
                tracing::debug!(
                    ?config,
                    "opening '{}' repository via address",
                    remote_name.as_ref()
                );
                config.open().await
            }
            Some(Remote::Config(config)) => {
                tracing::debug!(
                    ?config,
                    "opening '{}' repository via config",
                    remote_name.as_ref()
                );
                config.open().await
            }
            None => {
                return Err(crate::Error::UnknownRemoteName(
                    remote_name.as_ref().to_owned(),
                ));
            }
        }
        .map_err(|source| crate::Error::FailedToOpenRepository {
            repository: remote_name.as_ref().to_owned(),
            source,
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
        // Note: if a var using single underscores is set, it will have precedence
        .add_source(
            Environment::with_prefix("SPFS")
                .prefix_separator("_")
                .separator("__"),
        )
        // for backwards compatibility with vars not using double underscores
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
        Err(source) => Err(crate::Error::FailedToOpenRepository {
            repository: address.as_ref().to_owned(),
            source,
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

    if let Err(crate::Error::UnknownRemoteName(specifier)) = &rh {
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

    // No fallbacks worked so return the original result.
    rh
}
