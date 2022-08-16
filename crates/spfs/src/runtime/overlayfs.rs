// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::MetadataExt;

pub fn is_removed_entry(meta: &std::fs::Metadata) -> bool {
    // overlayfs uses character device files to denote
    // a file that was removed, using this special file
    // as a whiteout file of the same name.
    if meta.mode() & libc::S_IFCHR == 0 {
        return false;
    }
    // - the device is always 0/0 for a whiteout file
    meta.rdev() == 0
}
