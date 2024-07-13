// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use encoding::Digestible;
use rstest::rstest;

use super::EntryBuf;
use crate::encoding::{self};
use crate::fixtures::*;
use crate::tracking::{self, EntryKind};

#[rstest(entry, digest,
    case(
        EntryBuf::build_with_legacy_size(
            "testcase",
            EntryKind::Tree,
            0o40755,
            36,
            &"K53HFSBQEYR4SVFIDUGT63IE233MAMVKBQFXLA7M6HCR2AEMKIJQ====".parse().unwrap(),
        ),
        "VTTVI5AZULVVVIWRQMWKJ67TUAGWIECAS2GVTA7Q2QINS4XK4HQQ====".parse().unwrap(),
    ),
    case(
        EntryBuf::build(
            "swig_full_names.xsl",
            EntryKind::Blob(3293),
            0o100644,
            &"ZD25L3AN5E3LTZ6MDQOIZUV6KRV5Y4SSXRE4YMYZJJ3PXCQ3FMQA====".parse().unwrap(),
        ),
        "GP7DYE22DYLH3I5MB33PW5Z3AZXZIBGOND7MX65KECBMHVMXBUHQ====".parse().unwrap(),
    ),
)]
fn test_entry_encoding_compat(entry: EntryBuf, digest: encoding::Digest) {
    init_logging();

    let actual_digest = entry.as_entry().digest().unwrap();
    assert_eq!(
        actual_digest, digest,
        "expected encoding to match existing result"
    );
}

#[rstest]
fn test_entry_blobs_compare_name() {
    let a = EntryBuf::build(
        "a",
        tracking::EntryKind::Blob(0),
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let b = EntryBuf::build(
        "b",
        tracking::EntryKind::Blob(0),
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
        &encoding::EMPTY_DIGEST.into(),
    );
    let b = EntryBuf::build(
        "b",
        tracking::EntryKind::Tree,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    assert!(a.as_entry() < b.as_entry());
    assert!(b.as_entry() > a.as_entry());
}

#[rstest]
fn test_entry_mask_compare_name() {
    let a = EntryBuf::build(
        "a",
        tracking::EntryKind::Mask,
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let b = EntryBuf::build(
        "b",
        tracking::EntryKind::Mask,
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
        tracking::EntryKind::Blob(0),
        0,
        &encoding::EMPTY_DIGEST.into(),
    );
    let tree = EntryBuf::build(
        "b",
        tracking::EntryKind::Tree,
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
        tracking::EntryKind::Blob(0),
        0,
        &encoding::NULL_DIGEST.into(),
    );
    let root_dir = EntryBuf::build(
        "xdir",
        tracking::EntryKind::Tree,
        0,
        &encoding::NULL_DIGEST.into(),
    );
    assert!(root_dir.as_entry() > root_file.as_entry());
}
