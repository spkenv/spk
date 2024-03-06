// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::TreeBuf;
use crate::encoding;
use crate::encoding::prelude::*;
use crate::fixtures::*;
use crate::graph::entry::EntryBuf;
use crate::tracking::EntryKind;

#[rstest]
#[case(
    TreeBuf::build(vec![
        EntryBuf::build_with_legacy_size(
            "pkg",
            EntryKind::Tree,
            0o40755,
            1,
            &"CLWYXZVIKLJ2YUQC32ZLMZGDCVUOL577YAMRABDTZYTJNU6O6SYA====".parse().unwrap(),
        ),
    ]),
    "CHDST3RTAIOJWFBV3OXFB2QZ4U5FTCCJIZXPEQJFCESZMIFZSPTA====".parse().unwrap()
)]
#[case(
    TreeBuf::build(vec![
        EntryBuf::build(
            ".helmignore",
            EntryKind::Blob(342),
            0o100644,
            &"NJDVDBWMXKU2BJG6L2LKLS3N3T47VUGXNPBY4BCHBLEEIRNSDILA====".parse().unwrap(),
        ),
        EntryBuf::build(
            "Chart.yaml",
            EntryKind::Blob(911),
            0o100644,
            &"ULAX2BMLX3WKVI7YKRQLJEQDEWJRSDPCFPZPGBJCIQZJ4FIVZIKA====".parse().unwrap(),
        ),
        EntryBuf::build_with_legacy_size(
            "templates",
            EntryKind::Tree,
            0o40755,
            1,
            &"6LMAERTKGND5WAA4VQQLDAVPXZIHGBOVUCVS7WEGHQIZPWSRJ5FA====".parse().unwrap(),
        ),
        EntryBuf::build(
            "values.yaml",
            EntryKind::Blob(1699),
            0o100644,
            &"IZFXS6UQJTHYBVYK3KYPPZC3FYX6NL3L3MWXAJUULAJMFTGZPODQ====".parse().unwrap(),
        ),
    ]),
    "KP7FNGMD5XRT5KGZRDT5R33M3BGFS2SJG5DHFKJV3KKWZG3AGVXA====".parse().unwrap(),
)]
fn test_tree_encoding_compat(#[case] tree: TreeBuf, #[case] digest: encoding::Digest) {
    init_logging();

    let actual_digest = tree.as_tree().digest().unwrap();
    assert_eq!(
        actual_digest, digest,
        "expected encoding to match existing result"
    );
}
