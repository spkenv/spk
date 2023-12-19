// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use encoding::Digestible;
use rstest::rstest;

use super::EntryBuf;
use crate::encoding::{self};
use crate::fixtures::*;
use crate::tracking::EntryKind;

#[rstest(entry, digest,
    case(
        EntryBuf::build(
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
            EntryKind::Blob,
            0o100644,
            3293,
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
