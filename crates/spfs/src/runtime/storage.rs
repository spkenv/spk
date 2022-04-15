// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

///! Local file system storage of runtimes.
use std::ffi::OsStr;
use std::io::{BufReader, BufWriter, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{csh_exp, startup_csh, startup_sh};
use crate::encoding;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./storage_test.rs"]
mod storage_test;

/// The location in spfs where shell files can be placed be sourced at startup
pub static STARTUP_FILES_LOCATION: &str = "/spfs/etc/spfs/startup.d";

/// Stores the configuration of a single runtime.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    name: String,
    stack: Vec<encoding::Digest>,
    editable: bool,
    running: bool,
    pid: Option<u32>,
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
    pub fn upgrade(mut runtime: Runtime) -> Result<Self> {
        let pid = std::process::id();
        if let Some(existing) = runtime.get_pid() {
            if existing == pid {
                return Err("Owned runtime was already instantiated in this process".into());
            } else {
                return Err("Runtime is already owned by another process".into());
            }
        }
        runtime.set_pid(pid)?;
        Ok(Self(runtime))
    }
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        let _ = self.0.set_running(false);
        if let Err(err) = self.0.delete() {
            tracing::warn!(?err, "Failed to clean up runtime data")
        }
    }
}

/// Represents an active spfs session.
///
/// The runtime contains the working files for a spfs
/// envrionment, the contained stack of read-only filesystem layers.
#[derive(Debug)]
pub struct Runtime {
    root: PathBuf,
    pub upper_dir: PathBuf,
    pub config_file: PathBuf,
    pub sh_startup_file: PathBuf,
    pub csh_startup_file: PathBuf,
    pub csh_expect_file: PathBuf,
    config: Config,
}

impl Runtime {
    const UPPER_DIR: &'static str = "/tmp/spfs-runtime/upper";
    const CONFIG_FILE: &'static str = "config.json";
    const SH_STARTUP_FILE: &'static str = "startup.sh";
    const CSH_STARTUP_FILE: &'static str = "startup.csh";
    const CSH_EXPECT_FILE: &'static str = "_csh.exp";

    /// Create a runtime to represent the data under 'root'.
    pub fn new<S: AsRef<Path>>(root: S) -> Result<Self> {
        let root = std::fs::canonicalize(root)?;
        let name = match root.file_name() {
            None => return Err("Invalid runtime path, has no filename".into()),
            Some(name) => match name.to_str() {
                None => {
                    return Err("Invalid runtime path, basename is not a valid utf-8 string".into())
                }
                Some(s) => s.to_string(),
            },
        };
        makedirs_with_perms(&root, 0o777)?;

        let mut rt = Self {
            upper_dir: PathBuf::from(Self::UPPER_DIR),
            config_file: root.join(Self::CONFIG_FILE),
            sh_startup_file: root.join(Self::SH_STARTUP_FILE),
            csh_startup_file: root.join(Self::CSH_STARTUP_FILE),
            csh_expect_file: root.join(Self::CSH_EXPECT_FILE),
            config: Config {
                name,
                ..Default::default()
            },
            root,
        };
        rt.read_config()?;
        Ok(rt)
    }

    pub fn name(&self) -> &str {
        self.config.name.as_ref()
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    /// Return the identifier for this runtime.
    pub fn reference(&self) -> &OsStr {
        self.root.file_name().expect("runtime path has no filename")
    }

    /// Mark this runtime as editable or not.
    ///
    /// An editable runtime is mounted with working directories
    /// that allow changes to be made to the runtime filesystem and
    /// committed back as layers.
    pub fn set_editable(&mut self, editable: bool) -> Result<()> {
        self.read_config()?;
        self.config.editable = editable;
        self.write_config()
    }

    /// Return true if this runtime is editable.
    ///
    /// An editable runtime is mounted with working directories
    /// that allow changes to be made to the runtime filesystem and
    /// committed back as layers.
    pub fn is_editable(&self) -> bool {
        self.config.editable
    }

    /// Mark this runtime as currently running or not.
    pub fn set_running(&mut self, running: bool) -> Result<()> {
        self.read_config()?;
        self.config.running = running;
        self.write_config()
    }

    /// Return true if this runtime is currently running.
    pub fn is_running(&self) -> bool {
        self.config.running
    }

    /// Mark the process that owns this runtime, this should be the spfs
    /// init process under which the target process is directly running.
    fn set_pid(&mut self, pid: u32) -> Result<()> {
        self.read_config()?;
        self.config.pid = Some(pid);
        self.write_config()
    }

    /// Return the pid of this runtime's init process, if any.
    pub fn get_pid(&self) -> Option<u32> {
        self.config.pid
    }

    /// Reset the config for this runtime to its default state.
    pub fn reset_stack(&mut self) -> Result<()> {
        self.config.stack.truncate(0);
        self.write_config()
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
            .map(|pat| gitignore::Pattern::new(pat.as_ref(), &self.upper_dir))
            .map(|res| match res {
                Err(err) => Err(Error::from(err)),
                Ok(pat) => Ok(pat),
            })
            .collect::<Result<Vec<gitignore::Pattern>>>()?;
        for entry in walkdir::WalkDir::new(&self.upper_dir) {
            let entry = entry?;
            let fullpath = entry.path();
            if fullpath == self.upper_dir {
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
        match std::fs::metadata(&self.upper_dir) {
            Ok(meta) => meta.size() != 0,
            Err(err) => {
                // Treating other error types as dirty is not strictly
                // accurate, but it is not worth the trouble of needing
                // to return an error from this function
                !matches!(err.kind(), std::io::ErrorKind::NotFound)
            }
        }
    }

    /// Remove all data pertaining to this runtime.
    pub fn delete(&self) -> Result<()> {
        tracing::debug!("cleaning up runtime: {:?}", &self.root.display());
        std::fs::remove_dir_all(&self.root)?;
        Ok(())
    }

    /// Return this runtime's current object stack.
    pub fn get_stack(&self) -> &Vec<encoding::Digest> {
        &self.config.stack
    }

    /// Push an object id onto this runtime's stack.
    ///
    /// This will update the configuration of the runtime,
    /// and change the overlayfs options, but not update
    /// any currently running environment automatically.
    pub fn push_digest(&mut self, digest: &encoding::Digest) -> Result<()> {
        let mut new_stack = Vec::with_capacity(self.config.stack.len() + 1);
        new_stack.push(*digest);
        for existing in self.config.stack.drain(..) {
            // we do not want the same layer showing up twice, one for
            // efficiency and two it causes errors in overlayfs so promote
            // any existing instance to the new top of the stack
            if &existing == digest {
                continue;
            }
            new_stack.push(existing);
        }
        self.config.stack = new_stack;
        self.write_config()
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    fn write_config(&self) -> Result<()> {
        let mut file = BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.config_file)?,
        );
        serde_json::to_writer(&mut file, &self.config)?;
        file.flush()?;
        file.get_ref().sync_all()?;
        Ok(())
    }

    fn read_config(&mut self) -> Result<&mut Config> {
        match std::fs::File::open(&self.config_file) {
            Ok(file) => {
                let config = serde_json::from_reader(BufReader::new(file))?;
                self.config = config;
                Ok(&mut self.config)
            }
            Err(err) => {
                if let std::io::ErrorKind::NotFound = err.kind() {
                    self.write_config()?;
                    Ok(&mut self.config)
                } else {
                    Err(err.into())
                }
            }
        }
    }
}

fn ensure_runtime<P: AsRef<Path>>(path: P) -> Result<Runtime> {
    if let Some(parent) = path.as_ref().parent() {
        makedirs_with_perms(&parent, 0o777)?;
    }
    // the actual runtime dir is for this user only and is created
    // with the normal permission mask
    if let Err(err) = std::fs::create_dir(&path) {
        match err.kind() {
            std::io::ErrorKind::AlreadyExists => (),
            _ => return Err(err.into()),
        }
    }
    let runtime = Runtime::new(&path)?;
    match makedirs_with_perms(&runtime.upper_dir, 0o777) {
        Ok(_) => (),
        Err(err) => {
            if let Some(libc::EROFS) = err.raw_os_error() {
                // this can fail if we try to establish a new runtime
                // from a non-editable runtime but is not fatal. It will
                // only become fatal if the mount fails for this runtime in spfs-enter
                // so we defer to that point
            } else {
                return Err(err);
            }
        }
    }

    // Capture the current $TMPDIR value here before it
    // is lost when entering the runtime later.
    let tmpdir_value_for_child_process = std::env::var("TMPDIR").ok();

    std::fs::write(
        &runtime.sh_startup_file,
        startup_sh::source(&tmpdir_value_for_child_process),
    )?;
    std::fs::write(
        &runtime.csh_startup_file,
        startup_csh::source(&tmpdir_value_for_child_process),
    )?;
    std::fs::write(&runtime.csh_expect_file, csh_exp::SOURCE)?;
    Ok(runtime)
}

/// Manages the on-disk storage of many runtimes.
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Initialize a new storage inside the given root directory.
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        makedirs_with_perms(&root, 0o777)?;
        let root = std::fs::canonicalize(&root)?;
        Ok(Self { root })
    }

    /// Remove a runtime forcefully, returning the removed runtime data.
    pub fn remove_runtime<R: AsRef<OsStr>>(&self, reference: R) -> Result<Runtime> {
        let runtime = self.read_runtime(reference.as_ref())?;
        runtime.delete()?;
        Ok(runtime)
    }

    /// Access a runtime in this storage.
    ///
    /// # Errors:
    /// - [`spfs::Error::UnknownRuntime`] if the named runtime does not exist
    /// - if there are filesystem errors while reading the runtime on disk
    pub fn read_runtime<R: AsRef<Path>>(&self, reference: R) -> Result<Runtime> {
        let runtime_dir = self.root.join(reference.as_ref());
        if std::fs::symlink_metadata(&runtime_dir).is_ok() {
            Runtime::new(runtime_dir)
        } else {
            Err(Error::UnknownRuntime(
                reference.as_ref().to_string_lossy().into(),
            ))
        }
    }

    /// Create a new runtime.
    pub fn create_runtime(&self) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let reference = OsStr::new(&uuid);
        self.create_named_runtime(reference)
    }

    /// create a new runtime that is owned by this process and
    /// will be deleted upon drop. This is useful mainly in testing.
    pub fn create_owned_runtime(&self) -> Result<OwnedRuntime> {
        let rt = self.create_runtime()?;
        OwnedRuntime::upgrade(rt)
    }

    pub fn create_named_runtime<R: AsRef<OsStr>>(&self, reference: R) -> Result<Runtime> {
        let runtime_dir = self.root.join(reference.as_ref());
        if std::fs::symlink_metadata(&runtime_dir).is_ok() {
            Err(format!("Runtime already exists: {:?}", reference.as_ref()).into())
        } else {
            ensure_runtime(runtime_dir)
        }
    }

    /// Iterate through all currently stored runtimes.
    pub fn iter_runtimes<'a>(&'a self) -> Box<dyn Iterator<Item = Result<Runtime>> + 'a> {
        let read_dir_result = std::fs::read_dir(&self.root);
        match read_dir_result {
            Ok(read_dir) => {
                let root = self.root.clone();
                Box::new(read_dir.into_iter().map(move |dir| match dir {
                    Ok(dir) => Runtime::new(root.join(dir.file_name())),
                    Err(err) => Err(err.into()),
                }))
            }
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => Box::new(Vec::new().into_iter()),
                _ => Box::new(vec![Err(err.into())].into_iter()),
            },
        }
    }
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
