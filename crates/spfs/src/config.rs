use config::{Config as ConfigBase, Environment, File};

use crate::{runtime, storage, Result};
use std::path::PathBuf;

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

static DEFAULT_STORAGE_ROOT: &str = "~/.local/share/spfs";
static FALLBACK_STORAGE_ROOT: &str = "/tmp/spfs";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Runtime {
    pub max_layers: usize,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { max_layers: 40 }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Storage {
    pub root: PathBuf,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            root: expanduser::expanduser(DEFAULT_STORAGE_ROOT)
                .unwrap_or_else(|_| PathBuf::from(FALLBACK_STORAGE_ROOT)),
        }
    }
}

impl Storage {
    /// Return the path to the local runtime storage.
    pub fn runtime_root(&self) -> PathBuf {
        self.root.join("runtimes")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Remote {
    pub address: url::Url,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub storage: Storage,
    pub runtime: Runtime,
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

    /// Get the local repository instance as configured.
    pub fn get_repository(&self) -> Result<storage::fs::FSRepository> {
        storage::fs::FSRepository::create(&self.storage.root)
    }

    /// Get the local runtime storage, as configured.
    pub fn get_runtime_storage(&self) -> Result<runtime::Storage> {
        runtime::Storage::new(self.storage.runtime_root())
    }

    /// Get a remote repostory by name or address.
    pub fn get_remote<S: AsRef<str>>(
        &self,
        name_or_address: S,
    ) -> Result<storage::RepositoryHandle> {
        let addr = match self.remote.get(name_or_address.as_ref()) {
            Some(remote) => remote.address.clone(),
            None => {
                if let Ok(addr) = url::Url::parse(name_or_address.as_ref()) {
                    addr
                } else {
                    url::Url::parse(format!("file:{}", name_or_address.as_ref()).as_str())
                        .map_err(|err| crate::Error::from(format!("{:?}", err)))?
                }
            }
        };
        tracing::debug!(addr = ?addr, "opening repository");
        storage::open_repository(addr)
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
    Ok(s.try_into()?)
}
