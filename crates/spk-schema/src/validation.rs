// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use std::path::PathBuf;
use std::str::FromStr;

use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
use spfs::tracking::{Diff, DiffMode};
use spk_schema_ident::Ident;

use crate::validators::{
    must_collect_all_files,
    must_install_something,
    must_not_alter_existing_files,
};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

/// A Validator validates packages after they have been built
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Validator {
    MustInstallSomething,
    MustNotAlterExistingFiles,
    MustCollectAllFiles,
}

impl Validator {
    /// Validate the set of changes to spfs according to this validator
    pub fn validate<Package, P>(
        &self,
        pkg: &Package,
        diffs: &[spfs::tracking::Diff],
        prefix: P,
    ) -> spk_schema_validators::Result<()>
    where
        Package: crate::Package,
        P: AsRef<std::path::Path>,
    {
        match self {
            Self::MustInstallSomething => must_install_something(diffs, prefix),
            Self::MustNotAlterExistingFiles => must_not_alter_existing_files(diffs, prefix),
            Self::MustCollectAllFiles => must_collect_all_files(
                pkg.ident(),
                pkg.components().iter().map(|c| &c.files),
                diffs,
            ),
        }
    }
}

/// The set of validators that are enabled by default
pub fn default_validators() -> Vec<Validator> {
    vec![
        Validator::MustInstallSomething,
        Validator::MustNotAlterExistingFiles,
        Validator::MustCollectAllFiles,
    ]
}

/// ValidationSpec configures how builds of this package
/// should be validated. The default spec contains all
/// recommended validators
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ValidationSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<Validator>,
}

impl ValidationSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Return the set of active validators based on this spec
    pub fn configured_validators(&self) -> HashSet<Validator> {
        HashSet::from_iter(
            default_validators()
                .into_iter()
                .filter(|validator| !self.disabled.contains(validator)),
        )
    }

    /// Helper for constructing more useful error messages from schema validator errors
    fn assemble_error_message(
        &self,
        error: spk_schema_validators::Error,
        files_to_packages: &HashMap<RelativePathBuf, Ident>,
        conflicting_packages: &HashMap<(String, String), HashSet<RelativePathBuf>>,
    ) -> String {
        match error {
            spk_schema_validators::Error::ExistingFileAltered(diffmode, filepath) => {
                let operation = match *diffmode {
                    DiffMode::Changed(a, b) => {
                        let mut changes: Vec<String> = Vec::new();
                        if a.mode != b.mode {
                            changes.push(format!("permissions: {:06o} => {:06o}", a.mode, b.mode));
                        }
                        if a.kind != b.kind {
                            changes.push(format!("kind: {} => {}", a.kind, b.kind));
                        }
                        if a.object != b.object {
                            changes.push(format!("digest: {} => {}", a.object, b.object));
                        }
                        if a.size != b.size {
                            changes.push(format!("size: {} => {} bytes", a.size, b.size));
                        }

                        format!("Changed [{}]", changes.join(", "))
                    }
                    DiffMode::Removed(_) => String::from("Removed"),
                    _ => String::from("Added or Unchanged"),
                };

                let mut message = format!("\"{}\" was {}", filepath, operation);

                // Work out if the files in conflict came from more
                // than one package
                let packages: Vec<(&(String, String), &HashSet<RelativePathBuf>)> =
                    conflicting_packages
                        .iter()
                        .filter(|(_ps, fs)| fs.contains(&filepath))
                        .collect();

                if packages.is_empty() {
                    // Then the file is only in a single package, not
                    // in a pair of conflicting packages.
                    let package = match files_to_packages.get(&filepath) {
                        Some(ident) => (*ident).clone(),
                        _ => {
                            // This should not happen
                            Ident::from_str("an unknown package, so something went wrong.").unwrap()
                        }
                    };
                    message.push_str(&format!(". It is from {package}"));
                } else {
                    let num_others = packages.iter().map(|(_ps, fs)| fs.len()).sum::<usize>() - 1;
                    if num_others > 0 {
                        message.push_str(&format!(
                            " (along with {num_others} more file{})",
                            if num_others == 1 { "" } else { "s" }
                        ));
                    }
                    let pkgs = packages
                        .iter()
                        .flat_map(|(ps, _fs)| Vec::from([ps.0.clone(), ps.1.clone()]))
                        .collect::<Vec<String>>();
                    message.push_str(&format!(
                        " in {} packages: {}",
                        pkgs.len(),
                        pkgs.join(" AND ")
                    ));
                }

                message
            }
            _ => error.to_string(),
        }
    }

    /// Validate the current set of spfs changes as a build of this package
    pub async fn validate_build_changeset<Package>(
        &self,
        package: &Package,
        files_to_packages: &HashMap<RelativePathBuf, Ident>,
        conflicting_packages: &HashMap<(String, String), HashSet<RelativePathBuf>>,
    ) -> Result<()>
    where
        Package: crate::Package,
    {
        static SPFS: &str = "/spfs";

        let mut diffs = spfs::diff(None, None).await?;

        // FIXME: this is only required because of a bug in spfs reset which
        // fails to handle permission-only changes on files...
        reset_permissions(&mut diffs, SPFS)?;

        for validator in self.configured_validators().iter() {
            if let Err(err) = validator.validate(package, &diffs, SPFS) {
                let err_message =
                    self.assemble_error_message(err, files_to_packages, conflicting_packages);
                return Err(
                    crate::foundation::ident_build::InvalidBuildError::new_error(format!(
                        "{validator:?}: {err_message}",
                    ))
                    .into(),
                );
            }
        }

        // Remove any "unchanged" entries from `diffs`; this list can be used
        // to ignore entries in the upperdir that would otherwise be captured
        // as changed by the build. For example, renaming a file to a
        // different name and back to its original name.
        diffs.retain(|diff| !diff.mode.is_unchanged());

        Ok(diffs)
    }
}

// Reset all file permissions in spfs if permissions is the
// only change for the given file
// NOTE(rbottriell): permission changes are not properly reset by spfs
// so we must deal with them manually for now
pub fn reset_permissions<P: AsRef<relative_path::RelativePath>>(
    diffs: &mut [spfs::tracking::Diff],
    prefix: P,
) -> Result<()> {
    use std::os::unix::prelude::PermissionsExt;

    for diff in diffs.iter_mut() {
        match &diff.mode {
            DiffMode::Unchanged(_) | DiffMode::Removed(_) | DiffMode::Added(_) => continue,
            DiffMode::Changed(a, b) => {
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
                let nonperm_change = (mode_change | 0o777) ^ 0o777;
                if nonperm_change != 0 {
                    continue;
                }
                if mode_change != 0 {
                    let perms = std::fs::Permissions::from_mode(a.mode);
                    let filename = diff
                        .path
                        .to_path(PathBuf::from(prefix.as_ref().to_string()));
                    std::fs::set_permissions(&filename, perms)
                        .map_err(|err| Error::FileWriteError(filename, err))?;
                }
                diff.mode = DiffMode::Unchanged(a.clone());
            }
        }
    }
    Ok(())
}
