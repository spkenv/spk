// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub fn is_removed_entry(meta: &std::fs::Metadata) -> bool {
    // WinFSP does not have a working directory that stores whiteout files (yet)
    // so this function always returns false
    false
}
