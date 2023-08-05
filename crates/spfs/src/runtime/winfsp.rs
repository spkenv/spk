// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub fn is_removed_entry(meta: &std::fs::Metadata) -> bool {
    // WinFSP does not have a working directory that stores whiteout files (yet)
    // so this function always returns false
    false
}
