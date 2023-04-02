// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{Entry, Tree};
use crate::encoding::{self, Digestible};
use crate::fixtures::*;
use crate::graph::Object;
use crate::tracking::EntryKind;

#[rstest(entries, digest,
    case(vec![
            Entry::new(
                "CLWYXZVIKLJ2YUQC32ZLMZGDCVUOL577YAMRABDTZYTJNU6O6SYA====".parse().unwrap(),
                EntryKind::Tree,
                0o40755,
                1,
                "pkg".into(),
            ),
        ],
        "CHDST3RTAIOJWFBV3OXFB2QZ4U5FTCCJIZXPEQJFCESZMIFZSPTA====".parse().unwrap()
    ),
    case(vec![
            Entry::new(
                "NJDVDBWMXKU2BJG6L2LKLS3N3T47VUGXNPBY4BCHBLEEIRNSDILA====".parse().unwrap(),
                EntryKind::Blob,
                0o100644,
                342,
                ".helmignore".into(),
            ),
            Entry::new(
                "ULAX2BMLX3WKVI7YKRQLJEQDEWJRSDPCFPZPGBJCIQZJ4FIVZIKA====".parse().unwrap(),
                EntryKind::Blob,
                0o100644,
                911,
                "Chart.yaml".into(),
            ),
            Entry::new(
                "6LMAERTKGND5WAA4VQQLDAVPXZIHGBOVUCVS7WEGHQIZPWSRJ5FA====".parse().unwrap(),
                EntryKind::Tree,
                0o40755,
                1,
                "templates".into(),
            ),
            Entry::new(
                "IZFXS6UQJTHYBVYK3KYPPZC3FYX6NL3L3MWXAJUULAJMFTGZPODQ====".parse().unwrap(),
                EntryKind::Blob,
                0o100644,
                1699,
                "values.yaml".into(),
            ),
        ],
        "KP7FNGMD5XRT5KGZRDT5R33M3BGFS2SJG5DHFKJV3KKWZG3AGVXA====".parse().unwrap()),
)]
fn test_tree_encoding_compat(entries: Vec<Entry>, digest: encoding::Digest) {
    init_logging();

    let mut tree = Tree::default();
    for entry in entries.into_iter() {
        tree.add(entry).unwrap();
    }

    let actual_digest = tree.digest().unwrap();
    assert_eq!(
        actual_digest, digest,
        "expected encoding to match existing result"
    );

    // Also check via `Object`
    let tree_object = Object::Tree(tree);
    let actual_digest = tree_object.digest().unwrap();
    assert_eq!(
        actual_digest, digest,
        "expected encoding to match existing result"
    );
}
