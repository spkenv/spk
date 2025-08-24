// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Definition and persistent storage of runtimes.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env::temp_dir;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;

#[cfg(windows)]
use super::startup_ps;
use super::{config_nu, env_nu};
#[cfg(unix)]
use super::{startup_csh, startup_sh};
use crate::encoding::Digest;
use crate::env::SPFS_DIR_PREFIX;
use crate::graph::object::Enum;
use crate::graph::{Annotation, AnnotationValue, KeyAnnotationValuePair};
use crate::prelude::*;
use crate::runtime::LiveLayer;
use crate::storage::RepositoryHandle;
use crate::storage::fs::DURABLE_EDITS_DIR;
use crate::{Error, Result, bootstrap, graph, storage, tracking};

#[cfg(test)]
#[path = "./storage_test.rs"]
mod storage_test;

/// The location in spfs where shell files can be placed be sourced at startup
pub const STARTUP_FILES_LOCATION: &str = "/spfs/etc/spfs/startup.d";

/// The environment variable that can be used to specify the runtime fs size
const SPFS_FILESYSTEM_TMPFS_SIZE: &str = "SPFS_FILESYSTEM_TMPFS_SIZE";

// For durable parameter of create_runtime()
#[cfg(test)]
const TRANSIENT: bool = false;

/// Data type for pairs of annotation keys and values
pub type KeyValuePair<'a> = (&'a str, &'a str);

/// An owned instance of [`KeyValuePair`].
pub type KeyValuePairBuf = (String, String);

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
            host_name: whoami::fallible::hostname()
                .unwrap_or_else(|_| format!("unk-{}", ulid::Ulid::new())),
            created: chrono::Local::now(),
        }
    }
}

/// Information about the current state of a runtime
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The set of layers that are being used in this runtime,
    /// where the first element is the bottom of the stack,
    /// and may be overridden by later elements higher in the stack
    pub stack: graph::Stack,
    /// Additional layers that were created automatically due to the stack
    /// being too large.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub(crate) flattened_layers: HashSet<Digest>,
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
    /// The location of the startup script for powershell-based shells
    #[serde(default)] // for backwards-compatibility with existing runtimes
    pub ps_startup_file: PathBuf,
    /// The location of the startup script for sh-based shells
    pub sh_startup_file: PathBuf,
    /// The location of the startup script for csh-based shells
    pub csh_startup_file: PathBuf,
    /// The location of the startup script for nushell-based shells
    pub nu_env_file: PathBuf,
    pub nu_config_file: PathBuf,
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
    pub durable: bool,
    /// List of live layers to add on top of the runtime's overlayfs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub live_layers: Vec<LiveLayer>,
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
    const PS_STARTUP_FILE: &'static str = "startup.ps1";
    const NU_ENV_FILE: &'static str = "env.nu";
    const NU_CONFIG_FILE: &'static str = "config.nu";
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
            ps_startup_file: temp_dir().join(Self::PS_STARTUP_FILE),
            nu_env_file: root.join(Self::NU_ENV_FILE),
            nu_config_file: root.join(Self::NU_CONFIG_FILE),
            runtime_dir: Some(root),
            tmpfs_size,
            mount_namespace: None,
            mount_backend: MountBackend::OverlayFsWithRenders,
            secondary_repositories: Vec::new(),
            durable: false,
            live_layers: Vec::new(),
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

    pub fn is_backend_fuse(&self) -> bool {
        self.mount_backend.is_fuse()
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
    strum::VariantNames,
    Serialize,
    Deserialize,
)]
pub enum MountBackend {
    /// Renders each layer to a folder on disk, before mounting
    /// the whole stack as lower directories in overlayfs. Edits
    /// are stored in the overlayfs upper directory.
    #[cfg_attr(unix, default)]
    OverlayFsWithRenders,
    /// Mounts a fuse filesystem as the lower directory to
    /// overlayfs, using the overlayfs upper directory for edits
    OverlayFsWithFuse,
    /// Mounts a fuse filesystem directly
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

    pub fn is_fuse(&self) -> bool {
        match self {
            MountBackend::OverlayFsWithRenders => false,
            MountBackend::OverlayFsWithFuse => true,
            MountBackend::FuseOnly => true,
            MountBackend::WinFsp => false,
        }
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

    pub fn is_backend_fuse(&self) -> bool {
        self.config.is_backend_fuse()
    }

    /// Whether to keep the runtime when the process is exits
    pub fn is_durable(&self) -> bool {
        self.config.durable
    }

    /// Set whether to keep the runtime when the process exits. Note:
    /// this does not change the runtime into a durable runtime. It is
    /// a helper method used during the process of changing the
    /// runtime into a durable runtime.
    pub fn set_durable(&mut self, value: bool) {
        self.config.durable = value;
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

/// A key-value pair for setting environment variables.
#[derive(Clone, Debug)]
pub struct EnvKeyValue(pub String, pub String);

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

    pub fn is_backend_fuse(&self) -> bool {
        self.data.is_backend_fuse()
    }

    pub fn is_durable(&self) -> bool {
        self.data.is_durable()
    }

    /// Store a list of arbitrary key-value string pairs in the runtime
    pub async fn add_annotations(
        &mut self,
        data: Vec<KeyValuePair<'_>>,
        size_limit: usize,
    ) -> Result<()> {
        tracing::debug!("adding list of {} Annotations [{}]", data.len(), size_limit);

        let mut annotations: Vec<KeyAnnotationValuePair> = Vec::with_capacity(data.len());
        for (key, value) in data {
            let annotation_value = if value.len() <= size_limit {
                AnnotationValue::string(value)
            } else {
                let digest = self
                    .storage
                    .create_blob_for_string(value.to_owned()) // TODO: avoid the copy
                    .await?;
                tracing::debug!("annotation too large for Layer, created Blob for it: {digest}");
                AnnotationValue::blob(digest)
            };

            annotations.push((key, annotation_value));
        }

        let layer = graph::Layer::new_with_annotations(annotations);
        self.storage.inner.write_object(&layer).await?;

        // The new annotation is added to the bottom of the runtime's stack
        let layer_digest = layer.digest()?;
        self.push_digest(layer_digest);

        tracing::debug!("pushed layer with list of annotations to storage: {layer_digest}.");
        Ok(())
    }

    /// Store an arbitrary key-value string pair in the runtime
    pub async fn add_annotation(
        &mut self,
        key: &str,
        value: &str,
        size_limit: usize,
    ) -> Result<()> {
        tracing::debug!(
            "adding Annotation: key: {} => value: {} [len: {} > {}]",
            key,
            value,
            value.len(),
            size_limit
        );

        let annotation_value = if value.len() <= size_limit {
            AnnotationValue::string(value)
        } else {
            let digest = self
                .storage
                .create_blob_for_string(value.to_owned())
                .await?;
            tracing::debug!("annotation too large for Layer, created Blob for it: {digest}");
            AnnotationValue::Blob(Cow::Owned(digest))
        };

        let layer = graph::Layer::new_with_annotation(key, annotation_value);
        self.storage.inner.write_object(&layer).await?;

        // The new annotation is added to the bottom of the runtime's stack
        let layer_digest = layer.digest()?;
        self.push_digest(layer_digest);

        tracing::debug!("pushed layer with annotation to storage: {layer_digest}.");
        Ok(())
    }

    /// Return the string value stored as annotation under the given key.
    pub async fn annotation(&self, key: &str) -> Result<Option<Cow<'_, str>>> {
        for digest in self.status.stack.iter_bottom_up() {
            if let Some(s) = self.storage.find_annotation(&digest, key).await? {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    /// Return all the string values that are stored as annotation under any key.
    pub async fn all_annotations(&self) -> Result<BTreeMap<String, String>> {
        let mut data = BTreeMap::new();
        for digest in self.status.stack.iter_bottom_up() {
            let pairs = self
                .storage
                .find_annotation_key_value_pairs(&digest)
                .await?;
            for (key, value) in pairs {
                data.insert(key, value);
            }
        }
        Ok(data)
    }

    /// Reset parts of the runtime's state so it can be reused in
    /// another process run.
    pub async fn reinit_for_reuse_and_save_to_storage(&mut self) -> Result<()> {
        // Reset the durable runtime's owner, monitor, and
        // namespace fields so the runtime can be rerun in future.
        self.status.owner = None;
        self.status.monitor = None;
        self.config.mount_namespace = None;

        self.save_state_to_storage().await
    }

    /// List of additional paths to mount on top of this Runtime's overlayfs
    pub fn live_layers(&self) -> &Vec<LiveLayer> {
        &self.config.live_layers
    }

    /// Prepares the runtime's layer stack for the live layers
    pub async fn prepare_live_layers(&mut self) -> Result<()> {
        // Any bind mount point destinations for live layers must
        // exist in the runtime stack before the live layers are used.
        self.ensure_extra_bind_mount_locations_exist().await
    }

    /// If there are any extra bind mounts in the live layers in the
    /// runtime, ensure that all of their mount directory location
    /// will exist in the runtime's /spfs filesytem by creating and
    /// adding a new layer to the runtime that contains all the
    /// directory paths.
    async fn ensure_extra_bind_mount_locations_exist(&mut self) -> Result<()> {
        let live_layers = self.live_layers();
        if !live_layers.is_empty() {
            // Make a layer that contains paths to all the mount locations.
            // This layer is added to the runtime so all the mount paths are
            // present for the extra mounts. This avoids having to check all
            // the other layers in the runtime to see which extra mounts
            // locations are missing. Only directory and file mounts are supported.
            let tmp_dir = TempDir::new().map_err(|err| Error::String(err.to_string()))?;
            let mut seen_dir_mounts = HashMap::new();

            for layer in live_layers {
                let injection_mounts = layer.bind_mounts();

                for extra_mount in injection_mounts {
                    let extra_mountpoint = match extra_mount.dest.strip_prefix(SPFS_DIR_PREFIX) {
                        Some(mp) => mp.to_string(),
                        None => extra_mount.dest.clone(),
                    };
                    let mountpoint = PathBuf::from(tmp_dir.path()).join(extra_mountpoint);
                    tracing::debug!("extra bind mount point: {:?}", mountpoint);

                    if extra_mount.src.is_dir() {
                        tracing::debug!("extra bind mount point is a dir");
                        std::fs::create_dir_all(mountpoint.clone()).expect(
                            "failed to make extra mount directory location: {mountpoint:?}",
                        );
                        seen_dir_mounts.insert(mountpoint.clone(), extra_mount);
                    } else if extra_mount.src.is_file() {
                        tracing::debug!("extra bind mount point is a file");
                        if let Some(parent) = mountpoint.parent() {
                            // Because extra mounts are bind mounted in order, if there
                            // is a directory mount in the list of dirs that have already
                            // been processed, its mount will clobber this file mount
                            // point's destination before the file can be mounted. This
                            // will cause its bind mount to fail, unless the source dir
                            // for the dir mount also contains a file of the same name.
                            if let Some(dir_mount) = seen_dir_mounts.get(&parent.to_path_buf()) {
                                let existing_file = dir_mount
                                    .src
                                    .join(mountpoint.as_path().file_name().unwrap());
                                tracing::debug!("file to test will be: {existing_file:?}");
                                if !existing_file.exists() {
                                    // This file's mount will fail because of the earlier
                                    // directory extra mount over the file's parent directory.
                                    return Err(Error::String(format!(
                                        "Invalid extra mount order: the file mount, {}, will fail because its destination is hidden by the earlier dir mount, {}, please reorder these extra mounts",
                                        extra_mount, dir_mount
                                    )));
                                }
                            }

                            std::fs::create_dir_all(parent).expect(
                                "failed to make extra mount file location's parent: {mountpoint:?}",
                            );
                        }
                        OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(mountpoint)
                            .expect("failed to make extra mount file location: {mountpoint:?}");
                    } else {
                        // Only dirs and files are supported in spfs as bind mounts
                        return Err(Error::String(format!(
                            "Invalid extra mount: its source '{}' is not a directory or a file",
                            extra_mount.src.display()
                        )));
                    }
                }
            }

            let manifest = crate::tracking::compute_manifest(tmp_dir.path()).await?;

            // This creates and saves the layer into the same repo as
            // the one the runtime is in.
            let layer: crate::graph::Layer = self
                .storage
                .inner
                .create_layer_from_manifest(&manifest)
                .await?;
            tracing::debug!("new layer saved with digest: {}", layer.digest()?);

            // TODO: do we want to tag this extra layer as well?
            // self.storage.push_tag(&tag_spec, &layer.digest()?).await?;
            self.push_digest(layer.digest()?);
        }
        Ok(())
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
            MountBackend::WinFsp => false,
        }
    }

    /// Push an object id onto this runtime's stack.
    ///
    /// This will update the configuration of the runtime,
    /// and change the overlayfs options, but not save the runtime or
    /// update any currently running environment. Returns false
    /// if adding the digest had no change to the runtime stack.
    pub fn push_digest(&mut self, digest: Digest) -> bool {
        self.status.stack.push(digest)
    }

    /// Generate a platform with all the layers from this runtime
    /// properly stacked.
    pub fn to_platform(&self) -> graph::Platform {
        let mut stack = self.status.stack.clone();
        stack.extend(self.status.flattened_layers.iter());
        stack.into()
    }

    /// Write out the startup script data to disk, ensuring
    /// that all required startup files are present in their
    /// defined location.
    pub fn ensure_startup_scripts(
        &self,
        environment_overrides_for_child_process: &[EnvKeyValue],
    ) -> Result<()> {
        #[cfg(unix)]
        std::fs::write(
            &self.config.sh_startup_file,
            startup_sh::source(environment_overrides_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.sh_startup_file.clone(), err))?;
        #[cfg(unix)]
        std::fs::write(
            &self.config.csh_startup_file,
            startup_csh::source(environment_overrides_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.csh_startup_file.clone(), err))?;
        std::fs::write(
            &self.config.nu_env_file,
            env_nu::source(tmpdir_value_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.nu_env_file.clone(), err))?;
        std::fs::write(
            &self.config.nu_config_file,
            config_nu::source(tmpdir_value_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.nu_config_file.clone(), err))?;
        #[cfg(windows)]
        std::fs::write(
            &self.config.ps_startup_file,
            startup_ps::source(environment_overrides_for_child_process),
        )
        .map_err(|err| Error::RuntimeWriteError(self.config.ps_startup_file.clone(), err))?;
        Ok(())
    }

    async fn ensure_lower_dir(&self) -> Result<()> {
        if let Err(err) = makedirs_with_perms(&self.config.lower_dir, 0o777) {
            return Err(Error::RuntimeWriteError(self.config.lower_dir.clone(), err));
        }
        Ok(())
    }

    /// Creates the upper dir and work dir for this runtime, if they do not exit.
    pub async fn ensure_upper_dirs(&self) -> Result<()> {
        let mut result = makedirs_with_perms(&self.config.upper_dir, 0o777);
        if let Err(err) = result {
            return Err(Error::RuntimeWriteError(self.config.upper_dir.clone(), err));
        }
        result = makedirs_with_perms(&self.config.work_dir, 0o777);
        if let Err(err) = result {
            return Err(Error::RuntimeWriteError(self.config.work_dir.clone(), err));
        }
        Ok(())
    }

    pub async fn ensure_required_directories(&self) -> Result<()> {
        self.ensure_lower_dir().await?;
        self.ensure_upper_dirs().await?;
        Ok(())
    }

    /// Sets up a durable upper dir (and work dir) for the runtime
    /// based on the runtime's name. This will error if the upper dir
    /// cannot be created or is already in use by another runtime.
    pub async fn setup_durable_upper_dir(&mut self) -> Result<PathBuf> {
        // The runtime's name is used in the identifying token in the
        // durable upper dir's root path. This is stored in the local
        // repo in a known location so they are durable across
        // invocations. The local repo is used because the durable_path
        // must not be on NFS or else any mount overlayfs operation
        // that uses it will fail.
        let name = String::from(self.name());
        let durable_path = self.storage.durable_path(name.clone()).await?;
        self.storage
            .check_upper_path_in_existing_runtimes(name, durable_path.clone())
            .await?;

        self.data.config.upper_dir = durable_path.join(Config::UPPER_DIR);
        // The workdir has to be in the same filesystem/path root
        // as the upperdir, for overlayfs.
        self.data.config.work_dir = durable_path.join(Config::WORK_DIR);
        Ok(durable_path)
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
    pub fn new<R: Into<storage::RepositoryHandle>>(inner: R) -> Result<Self> {
        let mut inner = inner.into();

        // Keep runtime tags out of the tag namespace. Allow `spfs runtime`
        // to view and operate on all runtimes on the host.
        inner.try_as_tag_mut()?.try_set_tag_namespace(None)?;

        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// The address of the underlying repository being used
    pub fn address(&self) -> Cow<'_, url::Url> {
        self.inner.address()
    }

    /// Remove a runtime forcefully
    ///
    /// This can break environments that are currently being used, and
    /// is generally not safe to call directly. Instead, use [`OwnedRuntime::delete`].
    pub async fn remove_runtime<S: AsRef<str>>(&self, name: S) -> Result<()> {
        // Remove the durable path associated with the runtime first, if there is one.
        let durable_path = self.durable_path(name.as_ref().to_string()).await?;
        if durable_path.exists() {
            // Removing the durable upper path requires elevated privileges so:
            // 'spfs-clean --remove-durable RUNTIME_NAME --runtime-storage STORAGE_URL'
            // is run to do it.
            let mut cmd = bootstrap::build_spfs_remove_durable_command(
                name.as_ref().to_string(),
                &self.inner.address(),
            )?
            .into_std();
            tracing::trace!("running: {cmd:?}");
            match cmd
                .status()
                .map_err(|err| {
                    Error::ProcessSpawnError(
                        "spfs-clean --remove-durable to remove durable runtime".to_owned(),
                        err,
                    )
                })?
                .code()
            {
                Some(0) => (),
                Some(code) => {
                    return Err(Error::String(format!(
                        "spfs-clean --remove-durable returned non-zero exit status: {code}"
                    )));
                }
                None => {
                    return Err(Error::String(
                        "spfs-clean --remove-durable failed unexpectedly with no return code"
                            .to_string(),
                    ));
                }
            }
        }

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
    pub async fn create_transient_runtime(&self) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let durable = false;
        let live_layers = Vec::new();
        self.create_named_runtime(uuid, durable, live_layers).await
    }

    /// Create a new runtime with a generated name
    pub async fn create_runtime(
        &self,
        durable: bool,
        live_layers: Vec<LiveLayer>,
    ) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        self.create_named_runtime(uuid, durable, live_layers).await
    }

    /// Create a new runtime that is owned by this process and
    /// will be deleted upon drop
    #[cfg(test)]
    pub async fn create_owned_runtime(&self) -> Result<OwnedRuntime> {
        let live_layers = Vec::new();
        let rt = self.create_runtime(TRANSIENT, live_layers).await?;
        OwnedRuntime::upgrade_as_owner(rt).await
    }

    /// Create a new blob payload to hold the given string value
    pub(crate) async fn create_blob_for_string(&self, payload: String) -> Result<Digest> {
        self.inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload.into_bytes())))
            .await
    }

    /// Returns the value from the first annotation object matching
    /// the given key that can be found starting from the given
    /// digest's spfs object.
    pub(crate) async fn find_annotation(
        &self,
        digest: &Digest,
        key: &str,
    ) -> Result<Option<Cow<str>>> {
        let mut digests_to_process: Vec<Digest> = vec![*digest];

        while !digests_to_process.is_empty() {
            let mut next_iter_digests: Vec<Digest> = Vec::new();
            for digest in digests_to_process.iter() {
                match self
                    .inner
                    .read_ref(digest.to_string().as_str())
                    .await
                    .map(|fo| fo.into_enum())
                {
                    Ok(Enum::Platform(platform)) => {
                        for reference in platform.iter_bottom_up() {
                            next_iter_digests.push(*reference);
                        }
                    }
                    Ok(Enum::Layer(layer)) => {
                        for entry in layer.annotations() {
                            let annotation: Annotation = entry.into();
                            if annotation.key() == key {
                                let value = self.find_annotation_value(&annotation).await?;
                                return Ok(Some(Cow::Owned(value.into_owned())));
                            }
                        }
                    }
                    Err(err) => {
                        tracing::trace!(?err, ?digest, "read_ref failed");
                    }
                    _ => {
                        // None of the other objects contain Annotation
                    }
                }
            }
            digests_to_process = std::mem::take(&mut next_iter_digests);
        }
        Ok(None)
    }

    /// Returns all key-value pairs from the annotation object that
    /// can be found starting from the given digest's spfs object.
    pub(crate) async fn find_annotation_key_value_pairs(
        &self,
        digest: &Digest,
    ) -> Result<Vec<KeyValuePairBuf>> {
        let mut key_value_pairs: Vec<KeyValuePairBuf> = Vec::new();

        let mut digests_to_process: Vec<Digest> = vec![*digest];
        while !digests_to_process.is_empty() {
            let mut next_iter_digests: Vec<Digest> = Vec::new();
            for digest in digests_to_process.iter() {
                match self
                    .inner
                    .read_object(*digest)
                    .await
                    .map(|fo| fo.into_enum())
                {
                    Ok(Enum::Platform(platform)) => {
                        for reference in platform.iter_bottom_up() {
                            next_iter_digests.push(*reference);
                        }
                    }
                    Ok(Enum::Layer(layer)) => {
                        for entry in layer.annotations() {
                            let annotation: Annotation = entry.into();
                            let key = annotation.key().to_string();
                            let value = self.find_annotation_value(&annotation).await?;
                            key_value_pairs.push((key, value.into_owned()));
                        }
                    }
                    Err(err) => {
                        tracing::trace!(?err, ?digest, "read_ref failed");
                    }
                    _ => {
                        // None of the other objects could contain
                        // pieces of Annotation
                    }
                }
            }
            digests_to_process = std::mem::take(&mut next_iter_digests);
        }
        Ok(key_value_pairs)
    }

    /// Return the value, as a string, from the given annotation,
    /// loading the value from the blob referenced by the digest if
    /// the value is stored in an external blob.
    async fn find_annotation_value<'a>(
        &self,
        annotation: &'a Annotation<'a>,
    ) -> Result<Cow<'a, str>> {
        let data = match annotation.value() {
            AnnotationValue::String(s) => s,
            AnnotationValue::Blob(digest) => {
                let blob = self.inner.read_blob(*digest).await?;
                let (mut payload, filename) = self.inner.open_payload(*blob.digest()).await?;
                let mut writer: Vec<u8> = vec![];
                tokio::io::copy(&mut payload, &mut writer)
                    .await
                    .map_err(|err| {
                        Error::StorageReadError(
                            "copy of annotation payload to string buffer",
                            filename,
                            err,
                        )
                    })?;
                Cow::Owned(String::from_utf8(writer).map_err(|err| Error::String(err.to_string()))?)
            }
        };
        Ok(data)
    }

    pub async fn durable_path(&self, name: String) -> Result<PathBuf> {
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
            let Ok(runtime) = runtime else {
                continue;
            };
            if sample_upper_dir == *runtime.upper_dir() {
                return Err(Error::RuntimeUpperDirAlreadyInUse {
                    upper_name,
                    runtime_name: runtime.name().to_string(),
                });
            }
        }
        Ok(())
    }

    /// Create a new runtime with a specific name that will be kept or
    /// not based on the given durable flag. If the runtime is kept,
    /// it will use a durable upper root path for its upper/work dirs.
    pub async fn create_named_runtime<S: Into<String>>(
        &self,
        name: S,
        durable: bool,
        live_layers: Vec<LiveLayer>,
    ) -> Result<Runtime> {
        let name = name.into();
        let runtime_tag = runtime_tag(RuntimeDataType::Metadata, &name)?;
        match self.inner.resolve_tag(&runtime_tag).await {
            Ok(_) => return Err(Error::RuntimeExists(name)),
            Err(Error::UnknownReference(_)) => {}
            Err(err) => return Err(err),
        }

        let mut rt = Runtime::new(name.clone(), self.clone());
        rt.data.config.durable = durable;
        if durable {
            // Keeping a runtime also activates a durable upperdir.
            rt.setup_durable_upper_dir().await?;
        }
        rt.config.live_layers = live_layers;

        self.save_runtime(&rt).await?;
        Ok(rt)
    }

    /// Save the state of the provided runtime for later retrieval.
    pub async fn save_runtime(&self, rt: &Runtime) -> Result<()> {
        let payload_tag = runtime_tag(RuntimeDataType::Payload, rt.name())?;
        let meta_tag = runtime_tag(RuntimeDataType::Metadata, rt.name())?;
        let platform = rt.to_platform();
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
///
/// Returns EINVAL if the path contains any parent dir components, ie '..')
pub fn makedirs_with_perms<P: AsRef<Path>>(dirname: P, perms: u32) -> std::io::Result<()> {
    use std::path::Component;

    let dirname = dirname.as_ref();
    #[cfg(unix)]
    let perms = std::fs::Permissions::from_mode(perms);
    #[cfg(windows)]
    // Avoid unused variable warning.
    let _perms = perms;

    if !dirname.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path must be absolute".to_string(),
        ));
    }
    if dirname
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path cannot contain '..' references".to_string(),
        ));
    }

    let mut to_create = Vec::new();
    let mut path = Some(dirname);
    while let Some(current) = path {
        // even though checking existence first is not
        // needed, it is required to trigger the automounter
        // in cases when the desired path is in that location
        match std::fs::symlink_metadata(current) {
            Ok(_) => break,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => {
                    to_create.push(current);
                    path = current.parent();
                    continue;
                }
                // will fail later on with a better error, but it appears to exist
                std::io::ErrorKind::PermissionDenied => break,
                _ => return Err(err),
            },
        }
    }
    while let Some(path) = to_create.pop() {
        if let Err(err) = std::fs::create_dir(path) {
            match err.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => return Err(err),
            }
        };
        // not fatal, so it's worth allowing things to continue
        // even though it could cause permission issues later on
        #[cfg(unix)]
        let _ = std::fs::set_permissions(path, perms.clone());
    }
    Ok(())
}

impl From<gitignore::Error> for Error {
    fn from(err: gitignore::Error) -> Self {
        Self::new(format!("invalid glob pattern: {err:?}"))
    }
}
