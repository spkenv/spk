// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use spfs_encoding as encoding;

use crate::{Error, Result};

#[cfg(test)]
#[path = "./overlayfs_test.rs"]
mod overlayfs_test;

/// The environment variable that can be used to specify the runtime fs size
const SPFS_FILESYSTEM_TMPFS_SIZE: &str = "SPFS_FILESYSTEM_TMPFS_SIZE";

/// Parameters for using the overlayfs filesystem in spfs
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
    /// Defaults to the value of `SPFS_FILESYSTEM_TMPFS_SIZE`. When empty,
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
}

impl Default for Config {
    fn default() -> Self {
        Self::from_root(&Path::new(Self::RUNTIME_DIR))
    }
}

impl Config {
    pub(crate) const RUNTIME_DIR: &'static str = "/tmp/spfs-runtime";
    const UPPER_DIR: &'static str = "upper";
    const LOWER_DIR: &'static str = "lower";
    const WORK_DIR: &'static str = "work";

    fn from_root<P: Into<PathBuf>>(root: P) -> Self {
        let root = root.into();
        let tmpfs_size = std::env::var(SPFS_FILESYSTEM_TMPFS_SIZE)
            .ok()
            .and_then(|v| if v.is_empty() { None } else { Some(v) });
        Self {
            upper_dir: root.join(Self::UPPER_DIR),
            lower_dir: root.join(Self::LOWER_DIR),
            work_dir: root.join(Self::WORK_DIR),
            runtime_dir: Some(root),
            tmpfs_size,
        }
    }
}

#[async_trait::async_trait]
impl super::FileSystem for Config {
    async fn mount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send,
    {
        todo!()
    }

    async fn remount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send,
    {
        todo!()
    }

    fn reset<S: AsRef<str>>(&self, paths: &[S]) -> Result<()> {
        let paths = paths
            .iter()
            .map(|pat| gitignore::Pattern::new(pat.as_ref(), &self.upper_dir))
            .map(|res| match res {
                Err(err) => Err(Error::from(err)),
                Ok(pat) => Ok(pat),
            })
            .collect::<Result<Vec<gitignore::Pattern>>>()?;
        for entry in walkdir::WalkDir::new(&self.upper_dir) {
            let entry =
                entry.map_err(|err| Error::RuntimeReadError(self.upper_dir.clone(), err.into()))?;
            let fullpath = entry.path();
            if fullpath == self.upper_dir {
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
                        std::fs::remove_dir_all(&fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.to_owned(), err))?;
                    } else {
                        std::fs::remove_file(&fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.to_owned(), err))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Return true if the upper dir of this runtime has changes.
    fn is_dirty(&self) -> bool {
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
}

/// True if the provided filesystem metadata identifies
/// a file which, if in an overlayfs upper dir, signifies
/// a file which was deleted.
pub fn is_removed_entry(meta: &std::fs::Metadata) -> bool {
    // overlayfs uses character device files to denote
    // a file that was removed, using this special file
    // as a whiteout file of the same name.
    if meta.mode() & libc::S_IFCHR == 0 {
        return false;
    }
    // - the device is always 0/0 for a whiteout file
    meta.rdev() == 0
}

/// Load the set of available mount options that can be passed
/// to overlayfs on this system
pub fn overlayfs_available_options() -> crate::Result<HashSet<String>> {
    let output = std::process::Command::new("/sbin/modinfo")
        .arg("overlay")
        .output()
        .map_err(|err| Error::process_spawn_error("/sbin/modinfo".into(), err, None))?;

    if output.status.code().unwrap_or(1) != 0 {
        return Err(Error::OverlayFSNotInstalled);
    }

    parse_modinfo_params(&mut BufReader::new(output.stdout.as_slice()))
}

/// Parses the available parameters from the output of `modinfo` for a kernel module
fn parse_modinfo_params<R: BufRead>(reader: &mut R) -> Result<HashSet<String>> {
    let mut params = HashSet::new();
    for line in reader.lines() {
        let line = line.map_err(|err| {
            Error::String(format!("Failed to read kernel module information: {err}"))
        })?;
        let param = match line.strip_prefix("parm:") {
            Some(remainder) => remainder.trim(),
            None => continue,
        };
        let name = match param.split_once(':') {
            Some((name, _remainder)) => name,
            None => param,
        };
        params.insert(name.to_owned());
    }

    Ok(params)
}
