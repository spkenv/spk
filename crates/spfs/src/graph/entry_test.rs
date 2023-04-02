// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Entry;
use crate::encoding::{self, Digestible};
use crate::fixtures::*;
use crate::tracking::EntryKind;

#[rstest(entry, digest,
    case(Entry::new(
        "K53HFSBQEYR4SVFIDUGT63IE233MAMVKBQFXLA7M6HCR2AEMKIJQ====".parse().unwrap(),
        EntryKind::Tree,
        0o40755,
        36,
        "testcase".into(),
    ),
    "VTTVI5AZULVVVIWRQMWKJ67TUAGWIECAS2GVTA7Q2QINS4XK4HQQ====".parse().unwrap()),
    case(Entry::new(
        "ZD25L3AN5E3LTZ6MDQOIZUV6KRV5Y4SSXRE4YMYZJJ3PXCQ3FMQA====".parse().unwrap(),
        EntryKind::Blob,
        0o100644,
        3293,
        "swig_full_names.xsl".into(),
    ),
    "GP7DYE22DYLH3I5MB33PW5Z3AZXZIBGOND7MX65KECBMHVMXBUHQ====".parse().unwrap()),
)]
fn test_entry_encoding_compat(entry: Entry, digest: encoding::Digest) {
    init_logging();

    let actual_digest = entry.digest().unwrap();
    assert_eq!(
        actual_digest, digest,
        "expected encoding to match existing result"
    );
}
