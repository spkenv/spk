// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::env::data_path;
use crate::{api, Result};

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Denotes an error during the build process.
#[derive(Debug)]
pub struct BuildError {
    pub message: String,
}

impl BuildError {
    pub fn new_error(format_args: std::fmt::Arguments) -> crate::Error {
        crate::Error::Build(Self {
            message: std::fmt::format(format_args),
        })
    }
}

// Reset all file permissions in spfs if permissions is the
// only change for the given file
// NOTE(rbottriell): permission changes are not properly reset by spfs
// so we must deal with them manually for now
pub fn reset_permissions<P: AsRef<relative_path::RelativePath>>(
    diffs: &mut Vec<spfs::tracking::Diff>,
    prefix: P,
) -> Result<()> {
    for diff in diffs.iter_mut() {
        if diff.mode != spfs::tracking::DiffMode::Changed {
            continue;
        }
        if let Some((a, b)) = &diff.entries {
            if a.size != b.size {
                continue;
            }
            if a.object != b.object {
                continue;
            }
            if a.kind != b.kind {
                continue;
            }
            let mode_change = a.mode ^ b.mode;
            let nonperm_change = (mode_change | 0o777) ^ 0o77;
            if mode_change != 0 && nonperm_change == 0 {
                let perms = std::fs::Permissions::from_mode(a.mode);
                std::fs::set_permissions(
                    diff.path
                        .to_path(std::path::PathBuf::from(prefix.as_ref().to_string())),
                    perms,
                )?;
                diff.mode = spfs::tracking::DiffMode::Unchanged;
            }
        }
    }
    Ok(())
}

/// Return the file path for the given source package's files.
pub fn source_package_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix)
}

/// Return the file path for the given build's spec.yaml file.
///
/// This file is created during a build and stores the full
/// package spec of what was built.
pub fn build_spec_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("spec.yaml")
}

/// Return the file path for the given build's options.json file.
///
/// This file is created during a build and stores the set
/// of build options used when creating the package
pub fn build_options_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("options.json")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn build_script_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("build.sh")
}
