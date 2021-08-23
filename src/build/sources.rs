// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use crate::Result;

#[cfg(test)]
#[path = "./sources_test.rs"]
mod sources_test;

/// Validate the set of diffs for a source package build.
///
/// # Errors:
///   - crate::Error::Collection: if any issues are identified in the changeset
pub fn validate_source_changeset<P: AsRef<relative_path::RelativePath>>(
    diffs: Vec<spfs::tracking::Diff>,
    source_dir: P,
) -> Result<()> {
    if diffs.len() == 0 {
        return Err(crate::SpkError::Collection(
            "No source files collected, source package would be empty".to_string(),
        ));
    }

    let mut source_dir = source_dir.as_ref();
    source_dir = source_dir.strip_prefix("/spfs").unwrap_or(source_dir);
    for diff in diffs.into_iter() {
        if diff.mode == spfs::tracking::DiffMode::Unchanged {
            continue;
        }
        if diff.path.starts_with(&source_dir) {
            // the change is within the source directory
            continue;
        }
        if source_dir.starts_with(&diff.path) {
            // the path is to a parent directory of the source path
            continue;
        }
        return Err(crate::SpkError::Collection(format!(
            "Invalid source file path found: {} (not under {})",
            &diff.path, source_dir
        )));
    }
    Ok(())
}
