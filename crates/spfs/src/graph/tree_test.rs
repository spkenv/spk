// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{Entry, Tree};
use crate::encoding::{self, Encodable};
use crate::graph::Object;
use crate::tracking::EntryKind;

use crate::fixtures::*;

#[rstest(entries, digest,
    case(vec![
            Entry{
                name: "pkg".into(),
                mode: 0o40755,
                size: 1,
                kind: EntryKind::Tree,
                object: "CLWYXZVIKLJ2YUQC32ZLMZGDCVUOL577YAMRABDTZYTJNU6O6SYA====".parse().unwrap(),
            },
        ],
        "CHDST3RTAIOJWFBV3OXFB2QZ4U5FTCCJIZXPEQJFCESZMIFZSPTA====".parse().unwrap()
    ),
    case(vec![
            Entry{
                name: ".helmignore".into(),
                mode: 0o100644,
                size: 342,
                kind: EntryKind::Blob,
                object: "NJDVDBWMXKU2BJG6L2LKLS3N3T47VUGXNPBY4BCHBLEEIRNSDILA====".parse().unwrap(),
            },
            Entry{
                name: "Chart.yaml".into(),
                mode: 0o100644,
                size: 911,
                kind: EntryKind::Blob,
                object: "ULAX2BMLX3WKVI7YKRQLJEQDEWJRSDPCFPZPGBJCIQZJ4FIVZIKA====".parse().unwrap(),
            },
            Entry{
                name: "templates".into(),
                mode: 0o40755,
                size: 1,
                kind: EntryKind::Tree,
                object: "6LMAERTKGND5WAA4VQQLDAVPXZIHGBOVUCVS7WEGHQIZPWSRJ5FA====".parse().unwrap(),
            },
            Entry{
                name: "values.yaml".into(),
                mode: 0o100644,
                size: 1699,
                kind: EntryKind::Blob,
                object: "IZFXS6UQJTHYBVYK3KYPPZC3FYX6NL3L3MWXAJUULAJMFTGZPODQ====".parse().unwrap(),
            },
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
