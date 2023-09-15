// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Entry;

#[rstest]
fn test_empty_dir_is_dir() {
    let entry = Entry::<()>::empty_dir_with_open_perms();
    assert!(entry.is_dir());
}

#[rstest]
fn test_empty_file_is_file() {
    let entry = Entry::<()>::empty_file_with_open_perms();
    assert!(entry.is_regular_file());
}

#[rstest]
fn test_entry_has_no_default() {
    // we do not want to have any "Default" impl for entry because
    // it would be ambiguous in terms of aligning mode bits and entry
    // kind. Instead, explicit creation methods exist for creating
    // entries of various types with reasonable default field values.
    static_assertions::assert_not_impl_all!(Entry<()>: Default);
}
