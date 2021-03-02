///! Local file system storage of runtimes.
use std::ffi::OsStr;
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
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct Config {
    stack: Vec<encoding::Digest>,
    editable: bool,
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
        makedirs_with_perms(&root, 0o777)?;

        let mut rt = Self {
            upper_dir: PathBuf::from(Self::UPPER_DIR),
            config_file: root.join(Self::CONFIG_FILE),
            sh_startup_file: root.join(Self::SH_STARTUP_FILE),
            csh_startup_file: root.join(Self::CSH_STARTUP_FILE),
            csh_expect_file: root.join(Self::CSH_EXPECT_FILE),
            config: Default::default(),
            root: root,
        };
        rt.read_config()?;
        Ok(rt)
    }

    pub fn root<'a>(&'a self) -> &'a Path {
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
        return self.config.editable;
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
            .into_iter()
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
                if pattern.is_excluded(&fullpath, is_dir) {
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
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => false,
                // this is not strictly accurate, but not worth the
                // trouble of needing to return an error from this function
                _ => true,
            },
        }
    }

    /// Remove all data pertaining to this runtime.
    pub fn delete(&self) -> Result<()> {
        std::fs::remove_dir_all(&self.root)?;
        Ok(())
    }

    /// Return this runtime's current object stack.
    pub fn get_stack<'a>(&'a self) -> &'a Vec<encoding::Digest> {
        &self.config.stack
    }

    /// Push an object id onto this runtime's stack.
    ///
    /// This will update the configuration of the runtime,
    /// and change the overlayfs options, but not update
    /// any currently running environment automatically.
    pub fn push_digest(&mut self, digest: &encoding::Digest) -> Result<()> {
        let mut stack = vec![digest.clone()];
        stack.append(&mut self.config.stack);
        self.config = Config {
            stack: stack,
            ..self.config
        };
        self.write_config()
    }

    pub fn get_config<'a>(&'a self) -> &'a Config {
        &self.config
    }

    fn write_config(&self) -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.config_file)?;
        serde_json::to_writer(file, &self.config)?;
        Ok(())
    }

    fn read_config<'a>(&'a mut self) -> Result<&'a mut Config> {
        match std::fs::File::open(&self.config_file) {
            Ok(file) => {
                let config = serde_json::from_reader(file)?;
                self.config = config;
                Ok(&mut self.config)
            }
            Err(err) => {
                if let std::io::ErrorKind::NotFound = err.kind() {
                    self.config = Config::default();
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
    makedirs_with_perms(&path, 0o777)?;
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
                return Err(err.into());
            }
        }
    }

    std::fs::write(&runtime.sh_startup_file, startup_sh::SOURCE)?;
    std::fs::write(&runtime.csh_startup_file, startup_csh::SOURCE)?;
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
    pub fn read_runtime<R: AsRef<OsStr>>(&self, reference: R) -> Result<Runtime> {
        let runtime_dir = self.root.join(reference.as_ref());
        if let Ok(_) = std::fs::symlink_metadata(&runtime_dir) {
            Runtime::new(runtime_dir)
        } else {
            Err(format!("runtime does not exist: {:?}", reference.as_ref()).into())
        }
    }

    // Create a new runtime.
    pub fn create_runtime(&self) -> Result<Runtime> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let reference = OsStr::new(&uuid);
        self.create_named_runtime(reference)
    }

    pub fn create_named_runtime<R: AsRef<OsStr>>(&self, reference: R) -> Result<Runtime> {
        let runtime_dir = self.root.join(reference.as_ref());
        if let Ok(_) = std::fs::symlink_metadata(&runtime_dir) {
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
                _ => return Box::new(vec![Err(err.into())].into_iter()),
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
                std::fs::create_dir(&path)?;
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
