// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use spfs::tracking::{Diff, DiffMode};
use spk_schema_foundation::env::data_path;
use spk_schema_foundation::spec_ops::FileMatcher;
use spk_schema_ident::BuildIdent;

use crate::{Error, Result};

// Tests for this module are in spk-schema/src/v0/validators_test.rs to avoid
// a cyclic crate dependency (the tests need spk_schema::v0).

/// Validates that all remaining build files are collected into at least one component
pub fn must_collect_all_files<'a, Files>(
    pkg: &BuildIdent,
    files: Files,
    diffs: &[Diff],
) -> Result<()>
where
    Files: IntoIterator<Item = &'a FileMatcher>,
{
    let mut diffs: Vec<_> = diffs.iter().filter(|d| d.mode.is_added()).collect();
    let data_path = data_path(pkg).to_path("/");
    for matcher in files.into_iter() {
        diffs.retain(|d| {
            let entry = match &d.mode {
                spfs::tracking::DiffMode::Unchanged(e) => e,
                spfs::tracking::DiffMode::Changed(_, e) => e,
                spfs::tracking::DiffMode::Added(e) => e,
                spfs::tracking::DiffMode::Removed(_) => return false,
            };
            let path = d.path.to_path("/");
            // either part of a component explicitly or implicitly
            // because it's a data file
            let is_explicit = matcher.matches(&path, entry.is_dir());
            let is_collected = is_explicit || path.starts_with(&data_path);
            !is_collected
        });
        if diffs.is_empty() {
            return Ok(());
        }
    }
    Err(Error::SomeFilesNotCollected(
        diffs.into_iter().map(|d| d.path.to_string()).collect(),
    ))
}

/// Validates that something was installed for the package
pub fn must_install_something<P: AsRef<Path>>(diffs: &[Diff], prefix: P) -> Result<()> {
    let changes = diffs
        .iter()
        .filter(|diff| !diff.mode.is_unchanged())
        .count();

    if changes == 0 {
        Err(Error::BuildMadeNoFilesToInstall(format!(
            "{:?}",
            prefix.as_ref()
        )))
    } else {
        Ok(())
    }
}

/// Validates that the install process did not change
/// a file that belonged to a build dependency
pub fn must_not_alter_existing_files<P: AsRef<Path>>(diffs: &[Diff], _prefix: P) -> Result<()> {
    for diff in diffs.iter() {
        match &diff.mode {
            DiffMode::Added(_) | DiffMode::Unchanged(_) => continue,
            DiffMode::Removed(e) => {
                if e.is_dir() {
                    continue;
                }
            }
            DiffMode::Changed(a, b) => {
                if a.is_dir() && b.is_dir() {
                    continue;
                }
            }
        }
        return Err(Error::ExistingFileAltered(
            Box::new(diff.mode.clone()),
            diff.path.clone(),
        ));
    }
    Ok(())
}
