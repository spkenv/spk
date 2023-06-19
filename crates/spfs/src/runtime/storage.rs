// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Definition and persistent storage of runtimes.

use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use super::{startup_csh, startup_sh};
use crate::encoding::{self, Encodable};
use crate::storage::fs::DURABLE_EDITS_DIR;
use crate::storage::RepositoryHandle;
use crate::{graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./storage_test.rs"]
mod storage_test;

/// The location in spfs where shell files can be placed be sourced at startup
pub const STARTUP_FILES_LOCATION: &str = "/spfs/etc/spfs/startup.d";

/// The environment variable that can be used to specify the runtime fs size
const SPFS_FILESYSTEM_TMPFS_SIZE: &str = "SPFS_FILESYSTEM_TMPFS_SIZE";

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
    /// Additional layers that were created automatically due to the stack
    /// being too large.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub(crate) flattened_layers: HashSet<encoding::Digest>,
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
    /// The location of the temporary filesystem holding the runtime
    ///
    /// A single set of configured paths can be used for runtime data
    /// as long as they all share this common root, because the in-memory
    /// filesystem will exist within the mount namespace for the runtime
    ///
    /// The temporary filesystem also ensures that the runtime leaves no
    /// working data behind when exiting
    pub runtime_dir: Option<PathBuf>,
    /// The size of the temporary filesystem being mounted for runtime data
    ///
    /// Defaults to the value of SPFS_FILESYSTEM_TMPFS_SIZE. When empty,
    /// tempfs limits itself to half of the RAM of the current
    /// machine. This has no effect when the runtime_dir is not provided.
    pub tmpfs_size: Option<String>,
    /// The location of the overlayfs upper directory for this runtime
    pub upper_dir: PathBuf,
    /// The location of the overlayfs lower directory for this runtime
    ///
    /// This is the lowest directory in the stack of filesystem layers
    /// and is usually empty. Especially in the case of an empty runtime
    /// we still need at least one layer for overlayfs and this is it.
    pub lower_dir: PathBuf,
    /// The location of the overlayfs working directory for this runtime
    ///
    /// The filesystem uses this working directory as needed so it should not
    /// be accessed or used by any other processes on the local machine
    pub work_dir: PathBuf,
    /// The location of the startup script for sh-based shells
    pub sh_startup_file: PathBuf,
    /// The location of the startup script for csh-based shells
    pub csh_startup_file: PathBuf,
    /// The location of the expect utility script used for csh-based shell environments
    /// \[DEPRECATED\] This field still exists for spk/spfs interop but is unused
    #[serde(skip_deserializing, default = "Config::default_csh_expect_file")]
    pub csh_expect_file: PathBuf,
    /// The name of the mount namespace this runtime is using, if known
    pub mount_namespace: Option<PathBuf>,
    /// The type of mount being used in this runtime
    #[serde(
        default,
        skip_serializing_if = "MountBackend::is_overlayfs_with_renders"
    )]
    pub mount_backend: MountBackend,
    /// Additional repositories being used to support this runtime
    ///
    /// Typically, these are only relevant for runtimes that can read
    /// data from multiple repositories on-the-fly (eg FUSE)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_repositories: Vec<url::Url>,

    /// Whether to keep the runtime around once the process using it exits.
    #[serde(default)]
    pub keep_runtime: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_root(Path::new(Self::RUNTIME_DIR))
    }
}

impl Config {
    const RUNTIME_DIR: &'static str = "/tmp/spfs-runtime";
    const UPPER_DIR: &'static str = "upper";
    const LOWER_DIR: &'static str = "lower";
    const WORK_DIR: &'static str = "work";
    const SH_STARTUP_FILE: &'static str = "startup.sh";
    const CSH_STARTUP_FILE: &'static str = ".cshrc";
    const DEV_NULL: &'static str = "/dev/null";

    /// Return a dummy value for the legacy csh_expect_file field.
    fn default_csh_expect_file() -> PathBuf {
        Self::DEV_NULL.into()
    }

    fn from_root<P: Into<PathBuf>>(root: P) -> Self {
        let root = root.into();
        let tmpfs_size = std::env::var(SPFS_FILESYSTEM_TMPFS_SIZE)
            .ok()
            .and_then(|v| if v.is_empty() { None } else { Some(v) });
        Self {
            upper_dir: root.join(Self::UPPER_DIR),
            lower_dir: root.join(Self::LOWER_DIR),
            work_dir: root.join(Self::WORK_DIR),
            sh_startup_file: root.join(Self::SH_STARTUP_FILE),
            csh_startup_file: root.join(Self::CSH_STARTUP_FILE),
            csh_expect_file: Self::default_csh_expect_file(),
            runtime_dir: Some(root),
            tmpfs_size,
            mount_namespace: None,
            mount_backend: MountBackend::OverlayFsWithRenders,
            secondary_repositories: Vec::new(),
            keep_runtime: false,
        }
    }

    #[cfg(test)]
    fn set_root<P: Into<PathBuf>>(&mut self, path: P) {
        let root = path.into();
        self.upper_dir = root.join(Self::UPPER_DIR);
        self.lower_dir = root.join(Self::LOWER_DIR);
        self.work_dir = root.join(Self::WORK_DIR);
        self.sh_startup_file = root.join(Self::SH_STARTUP_FILE);
        self.csh_startup_file = root.join(Self::CSH_STARTUP_FILE);
        self.runtime_dir = Some(root);
    }
}

/// Identifies a filesystem backend for spfs
#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
    strum::EnumVariantNames,
    Serialize,
    Deserialize,
)]
pub enum MountBackend {
    /// Renders each layer to a folder on disk, before mounting
    /// the whole stack as lower directories in overlayfs. Edits
    /// are stored in the overlayfs upper directory.
    #[cfg_attr(unix, default)]
    OverlayFsWithRenders,
    // Mounts a since fuse filesystem as the lower directory to
    // overlayfs, using the overlayfs upper directory for edits
    OverlayFsWithFuse,
    // Mounts a fuse filesystem directly
    FuseOnly,
    /// Leverages the win file system protocol system to present
    /// dynamic file system entries to runtime processes
    #[cfg_attr(windows, default)]
    WinFsp,
}

impl MountBackend {
    pub fn is_overlayfs_with_renders(&self) -> bool {
        matches!(self, Self::OverlayFsWithRenders)
    }

    pub fn is_overlayfs_with_with_fuse(&self) -> bool {
        matches!(self, Self::OverlayFsWithRenders)
    }

    pub fn is_fuse_only(&self) -> bool {
        matches!(self, Self::FuseOnly)
    }

    pub fn is_winfsp(&self) -> bool {
        matches!(self, Self::WinFsp)
    }

    /// Reports whether this mount backend requires that all
    /// data be synced to the local repository before being executed
    pub fn requires_localization(&self) -> bool {
        match self {
            Self::OverlayFsWithRenders => true,
            Self::OverlayFsWithFuse => false,
            Self::FuseOnly => false,
            Self::WinFsp => false,
        }
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

    pub fn upper_dir(&self) -> &PathBuf {
        &self.config.upper_dir
    }

    /// Whether to keep the runtime when the process is exits
    pub fn keep_runtime(&self) -> bool {
        self.config.keep_runtime
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
            Ok(()) => Ok(()),
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

    pub fn upper_dir(&self) -> &PathBuf {
        self.data.upper_dir()
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn keep_runtime(&self) -> bool {
        self.data.keep_runtime()
    }

    /// Clear all working changes in this runtime's upper dir
    pub fn reset_all(&self) -> Result<()> {
        self.reset(&["*"])
    }

    /// Remove working changes from this runtime's upper dir.
    ///
    /// If no paths are specified, nothing is done.
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
            let entry = entry.map_err(|err| {
                Error::RuntimeReadError(self.config.upper_dir.clone(), err.into())
            })?;
            let fullpath = entry.path();
            if fullpath == self.config.upper_dir {
                continue;
            }
            for pattern in paths.iter() {
                let is_dir = entry
                    .metadata()
                    .map_err(|err| Error::RuntimeReadError(entry.path().to_owned(), err.into()))?
                    .file_type()
                    .is_dir();
                if pattern.is_excluded(fullpath, is_dir) {
                    if is_dir {
                        std::fs::remove_dir_all(fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.to_owned(), err))?;
                    } else {
                        std::fs::remove_file(fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.to_owned(), err))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Return true if the upper dir of this runtime has changes.
    pub fn is_dirty(&self) -> bool {
        match self.config.mount_backend {
            MountBackend::OverlayFsWithFuse | MountBackend::OverlayFsWithRenders => {
                match std::fs::metadata(&self.config.upper_dir) {
                    #[cfg(unix)]
                    Ok(meta) => meta.size() != 0,
                    #[cfg(windows)]
                    Ok(meta) => meta.file_size() != 0,
                    Err(err) => {
                        // Treating other error types as dirty is not strictly
                        // accurate, but it is not worth the trouble of needing
                        // to return an error from this function
                        !matches!(err.kind(), std::io::ErrorKind::NotFound)
                    }
                }
            }
            MountBackend::FuseOnly => false,
            MountBackend::WinFsp => todo!(),
        }
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

    /// Generate a platform with all the layers from this runtime
    /// properly stacked.
    pub fn to_platform(&self) -> graph::Platform {
        let mut platform = graph::Platform {
            stack: self.status.stack.clone(),
        };
        platform.stack.extend(self.status.flattened_layers.iter());
        platform
    }

    /// Write out the startup script data to disk, ensuring
    /// that all required startup files are present in their
    /// defined location.
    pub fn ensure_startup_scripts(
        &self,
        tmpdir_value_for_child_process: Option<&String>,
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
        Ok(())
    }

    async fn ensure_lower_dir(&self) -> Result<()> {
        if let Err(err) = makedirs_with_perms(&self.config.lower_dir, 0o777) {
            return Err(err.wrap(format!("Failed to create {:?}", self.config.lower_dir)));
        }
        Ok(())
    }

    pub async fn ensure_required_directories(&self) -> Result<()> {
        self.ensure_lower_dir().await?;
        let mut result = makedirs_with_perms(&self.config.upper_dir, 0o777);
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
        self.storage.save_runtime(self).await
    }

    /// Update the runtime's lower_dir to a new unique directory.
    pub async fn rotate_lower_dir(&mut self) -> Result<()> {
        self.config.lower_dir = self
            .config
            .lower_dir
            .parent()
            .ok_or_else(|| Error::String("upper_dir had no parent directory".into()))?
            .join(format!("{}-{}", Config::LOWER_DIR, ulid::Ulid::new()));
        self.ensure_lower_dir().await?;
        Ok(())
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
        // remove the durable path associated with the runtime, if there is one.
        let durable_path = self.durable_path(name.as_ref().to_string()).await?;
        if durable_path.exists() {
            std::fs::remove_dir_all(durable_path.clone())
                .map_err(|err| Error::RuntimeWriteError(durable_path, err))?;
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
                    runtime: format!("{} in storage {}", name.as_ref(), self.address()),
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
                        runtime: format!("{} in storage {}", name.as_ref(), self.address()),
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

    /// Create a runtime with a generated name that will not be kept
    pub async fn create_runtime(&self) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let keep_runtime = false;
        self.create_named_runtime(uuid, keep_runtime).await
    }

    /// Create a new runtime with a generated name that will be kept
    /// or not based on the the given keep_runtime value.
    pub async fn create_runtime_with_keep_runtime(&self, keep_runtime: bool) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        self.create_named_runtime(uuid, keep_runtime).await
    }

    /// Create a new runtime that is owned by this process and
    /// will be deleted upon drop
    #[cfg(test)]
    pub async fn create_owned_runtime(&self) -> Result<OwnedRuntime> {
        let rt = self.create_runtime().await?;
        OwnedRuntime::upgrade_as_owner(rt).await
    }

    async fn durable_path(&self, name: String) -> Result<PathBuf> {
        match &*self.inner {
            RepositoryHandle::FS(repo) => {
                let mut upper_root_path = repo.root();
                upper_root_path.push(DURABLE_EDITS_DIR);
                upper_root_path.push(name);
                Ok(upper_root_path)
            }
            _ => Err(Error::DoesNotSupportDurableRuntimePath),
        }
    }

    async fn check_upper_path_in_existing_runtimes(
        &self,
        upper_name: String,
        upper_root_path: PathBuf,
    ) -> Result<()> {
        // If upper root name is already being used by another runtime
        // (on this machine) then undefined sharing and masking will
        // occur between the runtimes. We don't want that to happen, so
        // runtimes aren't allowed to use the same named upper dir paths.
        let mut runtimes = self.iter_runtimes().await;
        let sample_upper_dir = upper_root_path.join(Config::UPPER_DIR);
        while let Some(runtime) = runtimes.next().await {
            let Ok(runtime) = runtime else { continue; };
            if sample_upper_dir == *runtime.upper_dir() {
                return Err(Error::RuntimeUpperDirAlreadyInUse(
                    upper_name,
                    runtime.name().to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Create a new runtime with a specific name that will be kept or
    /// not based on the given keep_runtime flag. If the runtime is kept,
    /// it will use a durable upper root path for its upper/work dirs.
    pub async fn create_named_runtime<S: Into<String>>(
        &self,
        name: S,
        keep_runtime: bool,
    ) -> Result<Runtime> {
        let name = name.into();
        let runtime_tag = runtime_tag(RuntimeDataType::Metadata, &name)?;
        match self.inner.resolve_tag(&runtime_tag).await {
            Ok(_) => return Err(Error::RuntimeExists(name)),
            Err(Error::UnknownReference(_)) => {}
            Err(err) => return Err(err),
        }

        let mut rt = Runtime::new(name.clone(), self.clone());
        rt.data.config.keep_runtime = keep_runtime;
        if keep_runtime {
            // Keeping a runtime also activates a durable upperdir.
            // The runtime's name is used the identifying token in the
            // durable upper dir's root path, which is stored in the
            // local repo in a known location to make them durable
            // across invocations.
            let durable_path = self.durable_path(name.clone()).await?;
            self.check_upper_path_in_existing_runtimes(name, durable_path.clone())
                .await?;

            // The durable_path must not be on NFS or else the mount
            // operation will fail.
            rt.data.config.upper_dir = durable_path.join(Config::UPPER_DIR);
            // The workdir has to be in the same filesystem/path root
            // as the upperdir, for overlayfs.
            rt.data.config.work_dir = durable_path.join(Config::WORK_DIR);
        }

        self.save_runtime(&rt).await?;
        Ok(rt)
    }

    /// Save the state of the provided runtime for later retrieval.
    pub async fn save_runtime(&self, rt: &Runtime) -> Result<()> {
        let payload_tag = runtime_tag(RuntimeDataType::Payload, rt.name())?;
        let meta_tag = runtime_tag(RuntimeDataType::Metadata, rt.name())?;
        let platform: graph::Object = rt.to_platform().into();
        let platform_digest = platform.digest()?;
        let config_data = serde_json::to_string(&rt.data)?;
        let (_, config_digest) = tokio::try_join!(
            self.inner.write_object(&platform),
            self.inner
                .commit_blob(Box::pin(std::io::Cursor::new(config_data.into_bytes())),)
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
    #[cfg(unix)]
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
                #[cfg(unix)]
                let _ = std::fs::set_permissions(&path, perms.clone());
            }
        }
    }
    Ok(())
}

impl From<gitignore::Error> for Error {
    fn from(err: gitignore::Error) -> Self {
        Self::new(format!("invalid glob pattern: {err:?}"))
    }
}
