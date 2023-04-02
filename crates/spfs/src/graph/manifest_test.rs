// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Entry;
use crate::{encoding, tracking};

#[rstest]
fn test_entry_blobs_compare_name() {
    let a = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "a".to_string(),
    );
    let b = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "b".to_string(),
    );
    assert!(a < b);
    assert!(b > a);
}

#[rstest]
fn test_entry_trees_compare_name() {
    let a = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "a".to_string(),
    );
    let b = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "b".to_string(),
    );
    assert!(a < b);
    assert!(b > a);
}

#[rstest]
fn test_entry_compare_kind() {
    let blob = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "a".to_string(),
    );
    let tree = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Tree,
        0,
        0,
        "b".to_string(),
    );
    assert!(tree > blob);
    assert!(blob < tree);
}

#[rstest]
fn test_entry_compare() {
    let root_file = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Blob,
        0,
        0,
        "file".to_string(),
    );
    let root_dir = Entry::new(
        encoding::EMPTY_DIGEST.into(),
        tracking::EntryKind::Tree,
        0,
        0,
        "xdir".to_string(),
    );
    assert!(root_dir > root_file);
}
