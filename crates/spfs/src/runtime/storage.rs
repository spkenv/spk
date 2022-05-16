// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

///! Configuration and storage of runtimes.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use encoding::Encodable;
use futures::{Stream, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use super::{csh_exp, startup_csh, startup_sh};
use crate::{encoding, graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./storage_test.rs"]
mod storage_test;

/// The location in spfs where shell files can be placed be sourced at startup
pub static STARTUP_FILES_LOCATION: &str = "/spfs/etc/spfs/startup.d";

/// Information about the source of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub user_name: String,
    pub host_name: String,
}

impl Default for Author {
    fn default() -> Self {
        Self {
            user_name: whoami::username(),
            host_name: whoami::hostname(),
        }
    }
}

/// Information about the current state of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The set of layers that are being used in this runtime
    pub stack: Vec<encoding::Digest>,
    /// Whether or not this runtime is editable
    pub editable: bool,
    /// Whether of not this runtime is currently active
    pub running: bool,
    /// The id of the process that owns this runtime
    ///
    /// This process is responsible for monitoring the usage
    /// of this runtime and cleaning it up when completed
    pub pid: Option<u32>,
    /// The primary command to be executed in this runtime
    ///
    /// An empty command signifies that this runtime is being
    /// used to launch an interactive shell environment
    pub command: Vec<String>,
}

/// Configuration parameters for the execution of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The location of the overlayfs upper directory for this runtime
    pub upper_dir: PathBuf,
    /// The location of the startup script for sh-based shells
    pub sh_startup_file: PathBuf,
    /// The location of the startup script for csh-based shells
    pub csh_startup_file: PathBuf,
    /// The location of the expect utility script used for csh-based shell environments
    pub csh_expect_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_root(&Path::new(Self::RUNTIME_DIR))
    }
}

impl Config {
    const RUNTIME_DIR: &'static str = "/tmp/spfs-runtime";
    const UPPER_DIR: &'static str = "upper";
    const SH_STARTUP_FILE: &'static str = "startup.sh";
    const CSH_STARTUP_FILE: &'static str = "startup.csh";
    const CSH_EXPECT_FILE: &'static str = "_csh.exp";

    fn from_root<P: AsRef<Path>>(root: P) -> Self {
        let root = root.as_ref();
        Self {
            upper_dir: root.join(Self::UPPER_DIR),
            sh_startup_file: root.join(Self::SH_STARTUP_FILE),
            csh_startup_file: root.join(Self::CSH_STARTUP_FILE),
            csh_expect_file: root.join(Self::CSH_EXPECT_FILE),
        }
    }

    #[cfg(test)]
    fn set_root<P: AsRef<Path>>(&mut self, path: P) {
        let runtime_dir = path.as_ref();
        self.upper_dir = runtime_dir.join(Self::UPPER_DIR);
        self.sh_startup_file = runtime_dir.join(Self::SH_STARTUP_FILE);
        self.csh_startup_file = runtime_dir.join(Self::CSH_STARTUP_FILE);
        self.csh_expect_file = runtime_dir.join(Self::CSH_EXPECT_FILE);
    }
}

/// Stores the complete information of a single runtime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Data {
    /// The name used to identify this runtime
    name: String,
    /// Information about the source of this runtime
    pub author: Author,
    /// The current state of this runtime (may change over time)
    pub status: Status,
    /// Parameters for this runtime's execution (should not change over time)
    pub config: Config,
}

impl Data {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            status: Default::default(),
            author: Default::default(),
            config: Default::default(),
        }
    }

    /// The unique name used to identify this runtime
    pub fn name(&self) -> &String {
        &self.name
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
    pub async fn upgrade(mut runtime: Runtime) -> Result<Self> {
        let pid = std::process::id();
        if let Some(existing) = runtime.get_pid() {
            if existing == pid {
                return Err("Owned runtime was already instantiated in this process".into());
            } else {
                return Err("Runtime is already owned by another process".into());
            }
        }
        runtime.set_pid(pid).await?;
        Ok(Self(runtime))
    }

    /// Remove all data pertaining to this runtime.
    pub async fn delete(self) -> Result<()> {
        tracing::debug!("cleaning up runtime: {}", &self.name());
        match self.0.storage.remove_runtime(self.name()).await {
            Ok(()) => Ok(()),
            Err(Error::UnknownRuntime(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// Represents an active spfs session.
///
/// The runtime contains the working files for a spfs
/// envrionment, the contained stack of read-only filesystem layers.
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
    /// be forgotten if not otheriwse modified or saved.
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

    /// Mark this runtime as editable or not.
    ///
    /// An editable runtime is mounted with working directories
    /// that allow changes to be made to the runtime filesystem and
    /// committed back as layers.
    pub async fn set_editable(&mut self, editable: bool) -> Result<()> {
        self.status.editable = editable;
        self.write_config().await
    }

    /// Return true if this runtime is editable.
    ///
    /// An editable runtime is mounted with working directories
    /// that allow changes to be made to the runtime filesystem and
    /// committed back as layers.
    pub fn is_editable(&self) -> bool {
        self.status.editable
    }

    /// Mark this runtime as currently running or not.
    pub async fn set_running(&mut self, running: bool) -> Result<()> {
        self.status.running = running;
        self.write_config().await
    }

    /// Return true if this runtime is currently running.
    pub fn is_running(&self) -> bool {
        self.status.running
    }

    /// Mark the process that owns this runtime, this should be the spfs
    /// init process under which the target process is directly running.
    async fn set_pid(&mut self, pid: u32) -> Result<()> {
        self.status.pid = Some(pid);
        self.write_config().await
    }

    /// Return the pid of this runtime's init process, if any.
    pub fn get_pid(&self) -> Option<u32> {
        self.status.pid
    }

    /// Reset the config for this runtime to its default state.
    pub async fn reset_stack(&mut self) -> Result<()> {
        self.status.stack.truncate(0);
        self.write_config().await
    }

    pub fn reset_all(&self) -> Result<()> {
        self.reset(&["*"])
    }

    /// Remove working changes from this runtime's upper dir.
    ///
    /// If no paths are specified, reset all changes.
    pub fn reset<S: AsRef<str>>(&self, paths: &[S]) -> Result<()> {
        let paths = paths
            .iter()
            .map(|pat| gitignore::Pattern::new(pat.as_ref(), &self.config.upper_dir))
            .map(|res| match res {
                Err(err) => Err(Error::from(err)),
                Ok(pat) => Ok(pat),
            })
            .collect::<Result<Vec<gitignore::Pattern>>>()?;
        for entry in walkdir::WalkDir::new(&self.config.upper_dir) {
            let entry = entry?;
            let fullpath = entry.path();
            if fullpath == self.config.upper_dir {
                continue;
            }
            for pattern in paths.iter() {
                let is_dir = entry.metadata()?.file_type().is_dir();
                if pattern.is_excluded(fullpath, is_dir) {
                    if is_dir {
                        std::fs::remove_dir_all(&fullpath)?;
                    } else {
                        std::fs::remove_file(&fullpath)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Return true if the upper dir of this runtime has changes.
    pub fn is_dirty(&self) -> bool {
        match std::fs::metadata(&self.config.upper_dir) {
            Ok(meta) => meta.size() != 0,
            Err(err) => {
                // Treating other error types as dirty is not strictly
                // accurate, but it is not worth the trouble of needing
                // to return an error from this function
                !matches!(err.kind(), std::io::ErrorKind::NotFound)
            }
        }
    }

    /// Return this runtime's current object stack.
    pub fn get_stack(&self) -> &Vec<encoding::Digest> {
        &self.status.stack
    }

    /// Push an object id onto this runtime's stack.
    ///
    /// This will update the configuration of the runtime,
    /// and change the overlayfs options, but not update
    /// any currently running environment automatically.
    pub async fn push_digest(&mut self, digest: &encoding::Digest) -> Result<()> {
        let mut new_stack = Vec::with_capacity(self.status.stack.len() + 1);
        new_stack.push(*digest);
        for existing in self.status.stack.drain(..) {
            // we do not want the same layer showing up twice, one for
            // efficiency and two it causes errors in overlayfs so promote
            // any existing instance to the new top of the stack
            if &existing == digest {
                continue;
            }
            new_stack.push(existing);
        }
        self.status.stack = new_stack;
        self.write_config().await
    }

    /// Write out the startup script data to disk, ensuring
    /// that all required startup files are present in their
    /// defined location.
    pub fn ensure_startup_scripts(&self) -> Result<()> {
        // Capture the current $TMPDIR value here before it
        // is lost when entering the runtime later.
        let tmpdir_value_for_child_process = std::env::var("TMPDIR").ok();

        std::fs::write(
            &self.config.sh_startup_file,
            startup_sh::source(&tmpdir_value_for_child_process),
        )?;
        std::fs::write(
            &self.config.csh_startup_file,
            startup_csh::source(&tmpdir_value_for_child_process),
        )?;
        std::fs::write(&self.config.csh_expect_file, csh_exp::SOURCE)?;
        Ok(())
    }

    /// Modify the root directory where runtime data is stored
    #[cfg(test)]
    pub async fn set_runtime_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.config.set_root(path);
        self.write_config().await
    }

    async fn write_config(&self) -> Result<()> {
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
    /// is generally not save to call directly. Instead, use [`OwnedRuntime::delete`].
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
    /// - [`spfs::Error::UnknownRuntime`] if the named runtime does not exist
    /// - if there are filesystem errors while reading the runtime on disk
    pub async fn read_runtime<R: AsRef<str>>(&self, name: R) -> Result<Runtime> {
        let tag_spec = runtime_tag(RuntimeDataType::Metadata, name.as_ref())?;
        let digest = match self.inner.resolve_tag(&tag_spec).await {
            Ok(tag) => tag.target,
            Err(Error::UnknownReference(_)) => {
                return Err(Error::UnknownRuntime(name.as_ref().to_string()))
            }
            Err(err) => return Err(err),
        };
        let mut reader = self.inner.open_payload(digest).await?;
        let mut data = String::new();
        reader.read_to_string(&mut data).await?;
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
        OwnedRuntime::upgrade(rt).await
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
        let platform: graph::Object = graph::Platform::new(&mut rt.get_stack().iter())?.into();
        let platform_digest = platform.digest()?;
        let config_data = serde_json::to_string(&rt.data)?;
        let (_, (config_digest, _)) = tokio::try_join!(
            self.inner.write_object(&platform),
            self.inner
                .write_data(Box::pin(std::io::Cursor::new(config_data.into_bytes())))
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
        // even though checking existance first is not
        // needed, it is required to trigger the automounter
        // in cases when the desired path is in that location
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(_) => {
                if let Err(err) = std::fs::create_dir(&path) {
                    match err.kind() {
                        std::io::ErrorKind::AlreadyExists => (),
                        _ => return Err(err.into()),
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
