// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Entry;
use crate::encoding::{self, Encodable};
use crate::fixtures::*;
use crate::tracking::EntryKind;

#[rstest(entry, digest,
    case(Entry{
        name: "testcase".into(),
        mode: 0o40755,
        size: 36,
        kind: EntryKind::Tree,
        object: "K53HFSBQEYR4SVFIDUGT63IE233MAMVKBQFXLA7M6HCR2AEMKIJQ====".parse().unwrap(),
    },
    "VTTVI5AZULVVVIWRQMWKJ67TUAGWIECAS2GVTA7Q2QINS4XK4HQQ====".parse().unwrap()),
    case(Entry{
        name: "swig_full_names.xsl".into(),
        mode: 0o100644,
        size: 3293,
        kind: EntryKind::Blob,
        object: "ZD25L3AN5E3LTZ6MDQOIZUV6KRV5Y4SSXRE4YMYZJJ3PXCQ3FMQA====".parse().unwrap(),
    },
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
