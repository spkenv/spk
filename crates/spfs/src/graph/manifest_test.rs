// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Manifest;
use crate::tracking::{self, EntryKind};

#[rstest]
fn test_manifest_from_tracking_manifest() {
    // Test a situation where the first entry in a tracking manifest
    // is a mask, the second entry is blob, the mask and blob do not
    // have the same name, and the manifest is converted to a graph
    // manifest. The manifest should not lose the blob in the
    // conversion.

    // Set up the manifest with a mask and file with different names.
    let mut p = tracking::Manifest::<()>::default();
    let node = p.mkfile("pip-21.2.3.dist-info").unwrap();
    node.kind = EntryKind::Mask;

    let mut t = tracking::Manifest::<()>::default();
    t.mkfile("typing_extensions.py").unwrap();

    let mut tm = tracking::Manifest::<()>::default();
    tm.update(&p);
    tm.update(&t);

    // Test convert to a graphing manifest, and back
    println!("tm: {:?}", tm);
    let gm: Manifest = tm.to_graph_manifest();
    println!("gm: {:?}", gm);
    let gm2tm = gm.to_tracking_manifest();
    println!("gm2tm: {:?}", gm2tm);

    assert!(tm == gm2tm);
}
