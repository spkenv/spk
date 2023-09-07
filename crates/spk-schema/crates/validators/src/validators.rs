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

/// The valiation type that discoved an a problem.
// XXX: This is a duplicate of a type in spk-schema.
#[derive(Debug)]
pub enum Validator {
    MustInstallSomething,
    MustNotAlterExistingFiles,
    MustCollectAllFiles,
}

/// The desired treatment of a discovered validation problem.
pub enum ValidationErrorFilterResult {
    /// Don't report this validation error as an error; keep looking for other
    /// validation problems.
    Continue,
    /// Report this validation error and stop the validation process.
    Stop,
}

/// Validates that all remaining build files are collected into at least one component
pub fn must_collect_all_files<'a, Files, F>(
    pkg: &BuildIdent,
    files: Files,
    diffs: &[Diff],
    validation_error_filter: F,
) -> Result<()>
where
    Files: IntoIterator<Item = &'a FileMatcher>,
    F: Fn(Validator, Option<&Diff>) -> ValidationErrorFilterResult,
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
    }
    for diff in diffs.iter() {
        if matches!(
            validation_error_filter(Validator::MustCollectAllFiles, Some(diff)),
            ValidationErrorFilterResult::Continue
        ) {
            continue;
        }

        return Err(Error::SomeFilesNotCollected(
            diffs.into_iter().map(|d| d.path.to_string()).collect(),
        ));
    }

    Ok(())
}

/// Validates that something was installed for the package
pub fn must_install_something<P, F>(
    diffs: &[Diff],
    prefix: P,
    validation_error_filter: F,
) -> Result<()>
where
    P: AsRef<Path>,
    F: Fn(Validator, Option<&Diff>) -> ValidationErrorFilterResult,
{
    let changes = diffs
        .iter()
        .filter(|diff| !diff.mode.is_unchanged())
        .count();

    if changes == 0
        && !matches!(
            validation_error_filter(Validator::MustInstallSomething, None),
            ValidationErrorFilterResult::Continue
        )
    {
        return Err(Error::BuildMadeNoFilesToInstall(format!(
            "{:?}",
            prefix.as_ref()
        )));
    }

    Ok(())
}

/// Validates that the install process did not change
/// a file that belonged to a build dependency
pub fn must_not_alter_existing_files<P, F>(
    diffs: &[Diff],
    _prefix: P,
    validation_error_filter: F,
) -> Result<()>
where
    P: AsRef<Path>,
    F: Fn(Validator, Option<&Diff>) -> ValidationErrorFilterResult,
{
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

        if matches!(
            validation_error_filter(Validator::MustNotAlterExistingFiles, Some(diff)),
            ValidationErrorFilterResult::Continue
        ) {
            continue;
        }

        return Err(Error::ExistingFileAltered(
            Box::new(diff.mode.clone()),
            diff.path.clone(),
        ));
    }
    Ok(())
}
