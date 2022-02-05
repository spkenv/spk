// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use spfs::tracking::{Diff, DiffMode};
use std::path::Path;

#[cfg(test)]
#[path = "./validators_test.rs"]
mod validators_test;

/// Validates that something was installed for the package
pub fn must_install_something<P: AsRef<Path>>(diffs: &[Diff], prefix: P) -> Option<String> {
    let changes = diffs
        .iter()
        .filter(|diff| diff.mode != DiffMode::Unchanged)
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
pub fn must_not_alter_existing_files<P: AsRef<Path>>(diffs: &[Diff], _prefix: P) -> Option<String> {
    for diff in diffs.iter() {
        if let Some((a, b)) = &diff.entries {
            if a.is_dir() && b.is_dir() {
                continue;
            }
        }
        match diff.mode {
            DiffMode::Added | DiffMode::Unchanged => continue,
            DiffMode::Removed => (),
            DiffMode::Changed => {
                if let Some((a, b)) = &diff.entries {
                    let mode_change = a.mode ^ b.mode;
                    let nonperm_change = (mode_change | 0o777) ^ 0o77;
                    if mode_change != 0 && nonperm_change == 0 {
                        continue;
                    }
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
