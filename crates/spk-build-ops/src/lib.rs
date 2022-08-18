// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;

use std::{os::unix::prelude::PermissionsExt, path::PathBuf};

pub use error::{Error, Result};

// Reset all file permissions in spfs if permissions is the
// only change for the given file
// NOTE(rbottriell): permission changes are not properly reset by spfs
// so we must deal with them manually for now
pub fn reset_permissions<P: AsRef<relative_path::RelativePath>>(
    diffs: &mut [spfs::tracking::Diff],
    prefix: P,
) -> Result<()> {
    use spfs::tracking::DiffMode;
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
                    std::fs::set_permissions(
                        diff.path
                            .to_path(PathBuf::from(prefix.as_ref().to_string())),
                        perms,
                    )?;
                }
                diff.mode = DiffMode::Unchanged(a.clone());
            }
        }
    }
    Ok(())
}
