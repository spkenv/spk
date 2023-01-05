// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::v0;

use super::{must_install_something, must_not_alter_existing_files};
use crate::validators::must_collect_all_files;

#[test]
fn test_validate_build_changeset_nothing() {
    let res = must_install_something(&[], "/spfs");
    assert!(res.is_some())
}

#[test]
fn test_validate_build_changeset_modified() {
    let res = must_not_alter_existing_files(
        &vec![spfs::tracking::Diff {
            path: "/spfs/file.txt".into(),
            mode: spfs::tracking::DiffMode::Changed(Default::default(), Default::default()),
        }],
        "/spfs",
    );
    assert!(res.is_some())
}

#[test]
fn test_validate_build_changeset_collected() {
    let mut spec = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    // the default components are added and collect all files,
    // so we remove them to ensure nothing is collected
    let _ = spec.install.components.drain(..);
    let res = must_collect_all_files(
        &spec.pkg,
        spec.install.components.iter().map(|c| &c.files),
        &vec![spfs::tracking::Diff {
            path: "/spfs/file.txt".into(),
            mode: spfs::tracking::DiffMode::Changed(Default::default(), Default::default()),
        }],
    );
    assert!(
        res.is_some(),
        "should get error when a file is created that was not in a component spec"
    )
}
