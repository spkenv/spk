// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::graph::entry::EntryBuf;
use crate::{encoding, tracking};

#[rstest]
fn test_entry_blobs_compare_name() {
    let a = EntryBuf::build(
        "a",
        tracking::EntryKind::Blob,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let b = EntryBuf::build(
        "b",
        tracking::EntryKind::Blob,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    assert!(a.as_entry() < b.as_entry());
    assert!(b.as_entry() > a.as_entry());
}

#[rstest]
fn test_entry_trees_compare_name() {
    let a = EntryBuf::build(
        "a",
        tracking::EntryKind::Tree,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let b = EntryBuf::build(
        "b",
        tracking::EntryKind::Tree,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    assert!(a.as_entry() < b.as_entry());
    assert!(b.as_entry() > a.as_entry());
}

#[rstest]
fn test_entry_compare_kind() {
    let blob = EntryBuf::build(
        "a",
        tracking::EntryKind::Blob,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let tree = EntryBuf::build(
        "b",
        tracking::EntryKind::Tree,
        0,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    assert!(tree.as_entry() > blob.as_entry());
    assert!(blob.as_entry() < tree.as_entry());
}

#[rstest]
fn test_entry_compare() {
    let root_file = EntryBuf::build(
        "file",
        tracking::EntryKind::Blob,
        0,
        0,
        &encoding::NULL_DIGEST.into(),
    );
    let root_dir = EntryBuf::build(
        "xdir",
        tracking::EntryKind::Tree,
        0,
        0,
        &encoding::NULL_DIGEST.into(),
    );
    assert!(root_dir.as_entry() > root_file.as_entry());
}
