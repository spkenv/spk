// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::Path;

use itertools::Itertools;
use spfs::tracking::{Diff, DiffMode};

use super::Spec;

#[cfg(test)]
#[path = "./validators_test.rs"]
mod validators_test;

/// Validates that all remaining build files are collected into at least one component
pub fn must_collect_all_files<P: AsRef<Path>>(
    spec: &Spec,
    diffs: &[Diff],
    _prefix: P,
) -> Option<String> {
    let mut diffs: Vec<_> = diffs.iter().filter(|d| d.mode.is_added()).collect();
    let data_path = crate::build::data_path(&spec.pkg).to_path("/");
    for component in spec.install.components.iter() {
        diffs = diffs
            .into_iter()
            .filter(|d| {
                let entry = match &d.mode {
                    spfs::tracking::DiffMode::Unchanged(e) => e,
                    spfs::tracking::DiffMode::Changed(_, e) => e,
                    spfs::tracking::DiffMode::Added(e) => e,
                    spfs::tracking::DiffMode::Removed(_) => return false,
                };
                let path = d.path.to_path("/");
                // either part of a component explicitly or implicitly
                // because it's a data file
                let is_explicit = component.files.matches(&path, entry.is_dir());
                let is_collected = is_explicit || path.starts_with(&data_path);
                !is_collected
            })
            .collect();
        if diffs.is_empty() {
            return None;
        }
    }
    Some(format!(
        "All generated files must be collected by a component. These ones were not: \n - {}",
        diffs.into_iter().map(|d| d.path.to_string()).join("\n - ")
    ))
}

/// Validates that something was installed for the package
pub fn must_install_something<P: AsRef<Path>>(
    _spec: &Spec,
    diffs: &[Diff],
    prefix: P,
) -> Option<String> {
    let changes = diffs
        .iter()
        .filter(|diff| !diff.mode.is_unchanged())
        .count();

    if changes == 0 {
        Some(format!(
            "Build process created no files under {:?}",
            prefix.as_ref(),
        ))
    } else {
        None
    }
}

/// Validates that the install process did not change
/// a file that belonged to a build dependency
pub fn must_not_alter_existing_files<P: AsRef<Path>>(
    _spec: &Spec,
    diffs: &[Diff],
    _prefix: P,
) -> Option<String> {
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
        return Some(format!(
            "Existing file was {:?}: {:?}",
            &diff.mode, &diff.path
        ));
    }
    None
}
