// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

///! Definition and persistent storage of runtimes.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use super::{csh_exp, startup_csh, startup_sh};
use crate::filesystem;
use crate::{
    encoding::{self, Encodable},
    graph, storage, tracking, Error, Result,
};

#[cfg(test)]
#[path = "./storage_test.rs"]
mod storage_test;

/// The location in spfs where shell files can be placed be sourced at startup
pub const STARTUP_FILES_LOCATION: &str = "/spfs/etc/spfs/startup.d";

/// Information about the source of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub user_name: String,
    pub host_name: String,
    pub created: chrono::DateTime<chrono::Local>,
}

impl Default for Author {
    fn default() -> Self {
        Self {
            user_name: whoami::username(),
            host_name: whoami::hostname(),
            created: chrono::Local::now(),
        }
    }
}

/// Information about the current state of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The set of layers that are being used in this runtime
    pub stack: Vec<encoding::Digest>,
    /// Whether or not this runtime is editable
    ///
    /// An editable runtime is mounted with working directories
    /// that allow changes to be made to the runtime filesystem and
    /// committed back as layers.
    pub editable: bool,
    /// Whether of not this runtime is currently active
    pub running: bool,
    /// The id of the process that owns this runtime
    ///
    /// This process was the original process spawned into
    /// the runtime and was the purpose for the runtime's creation
    pub owner: Option<u32>,
    /// The id of the process that is monitoring this runtime
    ///
    /// This process is responsible for monitoring the usage
    /// of this runtime and cleaning it up when completed
    pub monitor: Option<u32>,
    /// The primary command that was executed in this runtime
    ///
    /// An empty command signifies that this runtime is being
    /// used to launch an interactive shell environment
    pub command: Vec<String>,
}

/// Configuration parameters for the execution of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// For backwards-compatibility, the legacy overlayfs config
    /// can be mixed in with the runtime config
    #[deprecated(
        since = "0.34.7",
        note = "this is to support interoperability with older versions, use the runtime mount field instead"
    )]
    #[serde(flatten, default)]
    pub mount: Option<filesystem::overlayfs::Config>,

    /// The location of the startup script for sh-based shells
    pub sh_startup_file: PathBuf,
    /// The location of the startup script for csh-based shells
    pub csh_startup_file: PathBuf,
    /// The location of the expect utility script used for csh-based shell environments
    pub csh_expect_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_root(&Path::new(filesystem::overlayfs::Config::RUNTIME_DIR))
    }
}

impl Config {
    const SH_STARTUP_FILE: &'static str = "startup.sh";
    const CSH_STARTUP_FILE: &'static str = "startup.csh";
    const CSH_EXPECT_FILE: &'static str = "_csh.exp";

    fn from_root<P: Into<PathBuf>>(root: P) -> Self {
        let root = root.into();
        #[allow(deprecated, /*reason = "we are instantiating it's default value"*/)]
        Self {
            sh_startup_file: root.join(Self::SH_STARTUP_FILE),
            csh_startup_file: root.join(Self::CSH_STARTUP_FILE),
            csh_expect_file: root.join(Self::CSH_EXPECT_FILE),
            mount: None,
        }
    }

    #[cfg(test)]
    fn set_root<P: Into<PathBuf>>(&mut self, path: P) {
        let root = path.into();
        self.sh_startup_file = root.join(Self::SH_STARTUP_FILE);
        self.csh_startup_file = root.join(Self::CSH_STARTUP_FILE);
        self.csh_expect_file = root.join(Self::CSH_EXPECT_FILE);
        if let Some(m) = self.mount.as_mut() {
            m.set_root(root)
        }
    }
}

/// Stores the complete information of a single runtime.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Data {
    /// The name used to identify this runtime
    name: String,
    /// Information about the source of this runtime
    pub author: Author,
    /// The current state of this runtime (may change over time)
    pub status: Status,
    /// Parameters for this runtime's execution (should not change over time)
    pub config: Config,
    /// Parameters for this runtime's filesystem mount
    pub filesystem: filesystem::MountStrategy,
}

impl Data {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            status: Default::default(),
            author: Default::default(),
            config: Default::default(),
            filesystem: Default::default(),
        }
    }

    /// The unique name used to identify this runtime
    pub fn name(&self) -> &String {
        &self.name
    }
}

impl<'de> Deserialize<'de> for Data {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DataVisitor;

        impl<'de> serde::de::Visitor<'de> for DataVisitor {
            type Value = Data;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a runtime")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut name = Option::<String>::None;
                let mut author = Option::<Author>::None;
                let mut status = Option::<Status>::None;
                let mut config = Option::<Config>::None;
                let mut filesystem = Option::<filesystem::MountStrategy>::None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => name = map.next_value::<String>().map(Some)?,
                        "author" => author = map.next_value::<Author>().map(Some)?,
                        "status" => status = map.next_value::<Status>().map(Some)?,
                        "config" => config = map.next_value::<Config>().map(Some)?,
                        "filesystem" => {
                            filesystem = map.next_value::<filesystem::MountStrategy>().map(Some)?
                        }
                    }
                }

                let mut config = config.unwrap_or_default();
                #[allow(
                    deprecated,
                    /*reason = "for backwards-compatibility, we fall back to
                    embedded overlay options in the config"*/
                )]
                let fallback = config
                    .mount
                    .take()
                    .map(filesystem::MountStrategy::OverlayFS);
                Ok(Self::Value {
                    name: name.ok_or_else(|| serde::de::Error::missing_field("name"))?,
                    author: author.unwrap_or_default(),
                    status: status.unwrap_or_default(),
                    filesystem: filesystem.or(fallback).unwrap_or_default(),
                    config,
                })
            }
        }

        deserializer.deserialize_map(DataVisitor)
    }
}

#[derive(Debug)]
pub struct OwnedRuntime(Runtime);

impl std::ops::Deref for OwnedRuntime {
    type Target = Runtime;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl OwnedRuntime {
    /// Turn a runtime in to an owned runtime by associating the
    /// current process to it as the runtime's owner.
    ///
    /// The owner of a runtime is the primary entry point, or target
    /// command of the runtime. Only one process can own any given runtime
    /// and an error will returned if the provided runtime is already associated.
    pub async fn upgrade_as_owner(mut runtime: Runtime) -> Result<Self> {
        let pid = std::process::id();
        if let Some(existing) = &runtime.status.owner {
            if existing == &pid {
                return Err("Runtime was already upgraded by this process".into());
            } else {
                return Err("Runtime is already owned by another process".into());
            }
        }
        runtime.status.owner = Some(pid);
        runtime.status.running = true;
        runtime.save_state_to_storage().await?;
        Ok(Self(runtime))
    }

    /// Turn a runtime in to an owned runtime by associating the
    /// current process to it as the runtime's monitor.
    ///
    /// The monitor of a runtime is the process which is responsible for identifying
    /// when the runtime has completed and removing it from the storage. Only one
    /// process can monitor any given runtime and an error will returned if the
    /// provided runtime is already associated.
    pub async fn upgrade_as_monitor(mut runtime: Runtime) -> Result<Self> {
        let pid = std::process::id();
        if let Some(existing) = &runtime.status.monitor {
            if existing == &pid {
                return Err("Runtime was already upgraded by this process".into());
            } else {
                return Err("Runtime is already being monitored by another process".into());
            }
        }
        runtime.status.monitor = Some(pid);
        runtime.save_state_to_storage().await?;
        Ok(Self(runtime))
    }

    /// Remove all data pertaining to this runtime.
    pub async fn delete(self) -> Result<()> {
        tracing::debug!("cleaning up runtime: {}", &self.name());
        match self.0.storage.remove_runtime(self.name()).await {
            Ok(()) => {
                #[cfg(feature = "runtime-compat-0.33")]
                {
                    let config = crate::get_config()?;
                    let storage_root = config.storage.root.join("runtimes");
                    let storage = super::storage_033::Storage::new(storage_root)?;
                    match storage.remove_runtime(self.name()) {
                        Err(crate::Error::UnknownRuntime { .. }) => {}
                        Err(err) => {
                            tracing::warn!(?err, name=%self.name(), "failed to clean runtime from legacy storage")
                        }
                        Ok(_) => {}
                    };
                }
                Ok(())
            }
            Err(Error::UnknownRuntime { .. }) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// Represents an active spfs session.
///
/// The runtime contains the working files for a spfs
/// environment, the contained stack of read-only filesystem layers.
#[derive(Debug)]
pub struct Runtime {
    data: Data,
    storage: Storage,
}

impl std::ops::Deref for Runtime {
    type Target = Data;

    fn deref(&self) -> &Data {
        &self.data
    }
}

impl std::ops::DerefMut for Runtime {
    fn deref_mut(&mut self) -> &mut Data {
        &mut self.data
    }
}

impl Runtime {
    /// Create a runtime associated with the provided storage.
    ///
    /// The created runtime has not been saved and will
    /// be forgotten if not otherwise modified or saved.
    fn new<S>(name: S, storage: Storage) -> Self
    where
        S: Into<String>,
    {
        Self {
            data: Data::new(name),
            storage,
        }
    }

    /// The name of this runtime which identifies it uniquely
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Push an object id onto this runtime's stack.
    ///
    /// This will update the configuration of the runtime,
    /// and change the overlayfs options, but not save the runtime or
    /// update any currently running environment.
    pub fn push_digest(&mut self, digest: encoding::Digest) {
        let mut new_stack = Vec::with_capacity(self.status.stack.len() + 1);
        new_stack.push(digest);
        for existing in self.status.stack.drain(..) {
            // we do not want the same layer showing up twice, one for
            // efficiency and two it causes errors in overlayfs so promote
            // any existing instance to the new top of the stack
            if existing == digest {
                continue;
            }
            new_stack.push(existing);
        }
        self.status.stack = new_stack;
    }

    /// Write out the startup script data to disk, ensuring
    /// that all required startup files are present in their
    /// defined location.
    pub fn ensure_startup_scripts(
        &self,
        tmpdir_value_for_child_process: &Option<String>,
    ) -> Result<()> {
        std::fs::write(
            &self.config.sh_startup_file,
            startup_sh::source(tmpdir_value_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.sh_startup_file.clone(), err))?;
        std::fs::write(
            &self.config.csh_startup_file,
            startup_csh::source(tmpdir_value_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.csh_startup_file.clone(), err))?;
        std::fs::write(&self.config.csh_expect_file, csh_exp::SOURCE)
            .map_err(|err| Error::RuntimeWriteError(self.config.csh_expect_file.clone(), err))?;
        Ok(())
    }

    pub async fn ensure_required_directories(&self) -> Result<()> {
        let mut result = makedirs_with_perms(&self.config.lower_dir, 0o777);
        if let Err(err) = result {
            return Err(err.wrap(format!("Failed to create {:?}", self.config.lower_dir)));
        }
        result = makedirs_with_perms(&self.config.upper_dir, 0o777);
        if let Err(err) = result {
            return Err(err.wrap(format!("Failed to create {:?}", self.config.upper_dir)));
        }
        result = makedirs_with_perms(&self.config.work_dir, 0o777);
        if let Err(err) = result {
            return Err(err.wrap(format!("Failed to create {:?}", self.config.work_dir)));
        }
        Ok(())
    }

    /// Modify the root directory where runtime data is stored
    #[cfg(test)]
    pub async fn set_runtime_dir<P: Into<PathBuf>>(&mut self, path: P) -> Result<()> {
        self.config.set_root(path);
        self.save_state_to_storage().await
    }

    /// Reload the state of this runtime from the underlying storage
    pub async fn reload_state_from_storage(&mut self) -> Result<()> {
        let rt = self.storage.read_runtime(&self.name).await?;
        self.data = rt.data;
        Ok(())
    }

    /// Save the current state of this runtime to the underlying storage
    pub async fn save_state_to_storage(&self) -> Result<()> {
        #[cfg(feature = "runtime-compat-0.33")]
        {
            // Legacy storage location can be changed via $SPFS_STORAGE_RUNTIMES;
            // legacy spk utilizes this for its test suite.
            let storage_root = match std::env::var("SPFS_STORAGE_RUNTIMES") {
                Ok(override_path) => override_path.into(),
                Err(_) => {
                    let config = crate::get_config()?;
                    config.storage.root.join("runtimes")
                }
            };
            let storage = super::storage_033::Storage::new(storage_root)?;
            let mut replica = storage.read_runtime(self.name()).or_else(|err| match err {
                crate::Error::UnknownRuntime { .. } => storage.create_named_runtime(self.name()),
                _ => Err(err),
            })?;
            replica.replicate(self)?;
        }
        self.storage.save_runtime(self).await
    }
}

/// Manages the on-disk storage of many runtimes.
#[derive(Debug, Clone)]
pub struct Storage {
    inner: Arc<storage::RepositoryHandle>,
}

impl Storage {
    /// Initialize a new storage for the provided repository
    ///
    /// Runtime storage is expected to be backed by the same repository
    /// that will be used to render and run the environment.
    pub fn new<R: Into<Arc<storage::RepositoryHandle>>>(inner: R) -> Self {
        Self {
            inner: inner.into(),
        }
    }

    /// The address of the underlying repository being used
    pub fn address(&self) -> url::Url {
        self.inner.address()
    }

    /// Remove a runtime forcefully
    ///
    /// This can break environments that are currently being used, and
    /// is generally not safe to call directly. Instead, use [`OwnedRuntime::delete`].
    pub async fn remove_runtime<S: AsRef<str>>(&self, name: S) -> Result<()> {
        // a runtime with no data takes up very little space, so we
        // remove the payload tag first because the other case is having
        // a tagged payload but no associated metadata
        let tags = &[RuntimeDataType::Payload, RuntimeDataType::Metadata]
            .iter()
            .map(|dt| runtime_tag(*dt, name.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        for tag in tags {
            match self.inner.remove_tag_stream(tag).await {
                Ok(_) => {}
                Err(Error::UnknownReference(_)) => {}
                err => return err,
            }
        }
        Ok(())
    }

    /// Access a runtime in this storage
    ///
    /// # Errors:
    /// - [`Error::UnknownRuntime`] if the named runtime does not exist
    /// - if there are filesystem errors while reading the runtime on disk
    pub async fn read_runtime<R: AsRef<str>>(&self, name: R) -> Result<Runtime> {
        let tag_spec = runtime_tag(RuntimeDataType::Metadata, name.as_ref())?;
        let digest = match self.inner.resolve_tag(&tag_spec).await {
            Ok(tag) => tag.target,
            Err(err @ Error::UnknownReference(_)) => {
                return Err(Error::UnknownRuntime {
                    message: format!("{} in storage {}", name.as_ref(), self.address()),
                    source: Box::new(err),
                });
            }
            Err(err) => return Err(err),
        };
        let (mut reader, filename) =
            self.inner
                .open_payload(digest)
                .await
                .map_err(|err| match err {
                    Error::UnknownObject(_) => Error::UnknownRuntime {
                        message: format!("{} in storage {}", name.as_ref(), self.address()),
                        source: Box::new(err),
                    },
                    _ => err,
                })?;
        let mut data = String::new();
        reader
            .read_to_string(&mut data)
            .await
            .map_err(|err| Error::RuntimeReadError(filename, err))?;
        let config: Data = serde_json::from_str(&data)?;
        Ok(Runtime {
            data: config,
            storage: self.clone(),
        })
    }

    /// Create a new runtime
    pub async fn create_runtime(&self) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        self.create_named_runtime(uuid).await
    }

    /// Create a new runtime that is owned by this process and
    /// will be deleted upon drop
    #[cfg(test)]
    pub async fn create_owned_runtime(&self) -> Result<OwnedRuntime> {
        let rt = self.create_runtime().await?;
        OwnedRuntime::upgrade_as_owner(rt).await
    }

    /// Create a new, empty runtime with a specific name
    pub async fn create_named_runtime<S: Into<String>>(&self, name: S) -> Result<Runtime> {
        let name = name.into();
        let runtime_tag = runtime_tag(RuntimeDataType::Metadata, &name)?;
        match self.inner.resolve_tag(&runtime_tag).await {
            Ok(_) => return Err(Error::RuntimeExists(name)),
            Err(Error::UnknownReference(_)) => {}
            Err(err) => return Err(err),
        }
        let rt = Runtime::new(name, self.clone());
        self.save_runtime(&rt).await?;
        Ok(rt)
    }

    /// Save the state of the provided runtime for later retrieval
    pub async fn save_runtime(&self, rt: &Runtime) -> Result<()> {
        let payload_tag = runtime_tag(RuntimeDataType::Payload, rt.name())?;
        let meta_tag = runtime_tag(RuntimeDataType::Metadata, rt.name())?;
        let platform: graph::Object = graph::Platform::new(&mut rt.status.stack.iter())?.into();
        let platform_digest = platform.digest()?;
        let config_data = serde_json::to_string(&rt.data)?;
        let (_, config_digest) = tokio::try_join!(
            self.inner.write_object(&platform),
            self.inner
                .commit_blob(Box::pin(std::io::Cursor::new(config_data.into_bytes())))
        )?;

        tokio::try_join!(
            self.inner.push_tag(&meta_tag, &config_digest),
            self.inner.push_tag(&payload_tag, &platform_digest)
        )?;
        Ok(())
    }

    /// Iterate through all currently stored runtimes
    pub async fn iter_runtimes(&self) -> Pin<Box<dyn Stream<Item = Result<Runtime>> + Send>> {
        let storage = self.clone();
        Box::pin(
            self.inner
                .ls_tags(relative_path::RelativePath::new("spfs/runtimes/meta"))
                .filter(|entry| {
                    // Ignore things that aren't `Tag`s.
                    futures::future::ready(matches!(*entry, Ok(storage::EntryType::Tag(_))))
                })
                .and_then(move |name| {
                    let storage = storage.clone();
                    async move { storage.read_runtime(name).await }
                }),
        )
    }
}

/// Specifies a type of runtime data being stored
#[derive(Clone, Copy)]
enum RuntimeDataType {
    /// Runtime metadata is the actual configuration of the runtime
    Metadata,
    /// Runtime payload data identifies the spfs file data being used
    Payload,
}

impl std::fmt::Display for RuntimeDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metadata => "meta".fmt(f),
            Self::Payload => "data".fmt(f),
        }
    }
}

fn runtime_tag<S: std::fmt::Display>(
    data_type: RuntimeDataType,
    name: S,
) -> Result<tracking::TagSpec> {
    tracking::TagSpec::parse(format!("spfs/runtimes/{data_type}/{name}"))
}

/// Recursively create the given directory with the appropriate permissions.
pub fn makedirs_with_perms<P: AsRef<Path>>(dirname: P, perms: u32) -> Result<()> {
    let dirname = dirname.as_ref();
    let perms = std::fs::Permissions::from_mode(perms);
    let mut path = PathBuf::from("/");
    for component in dirname.components() {
        path = match component {
            std::path::Component::Normal(component) => path.join(component),
            std::path::Component::ParentDir => path
                .parent()
                .ok_or_else(|| {
                    Error::String(
                        "cannot traverse below root, too many '..' references".to_string(),
                    )
                })?
                .to_path_buf(),
            _ => continue,
        };
        // even though checking existence first is not
        // needed, it is required to trigger the automounter
        // in cases when the desired path is in that location
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(_) => {
                if let Err(err) = std::fs::create_dir(&path) {
                    match err.kind() {
                        std::io::ErrorKind::AlreadyExists => (),
                        _ => return Err(Error::RuntimeWriteError(path, err)),
                    }
                }
                // not fatal, so it's worth allowing things to continue
                // even though it could cause permission issues later on
                let _ = std::fs::set_permissions(&path, perms.clone());
            }
        }
    }
    Ok(())
}

impl From<gitignore::Error> for Error {
    fn from(err: gitignore::Error) -> Self {
        Self::new(format!("invalid glob pattern: {:?}", err))
    }
}
