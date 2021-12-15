// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use super::{must_install_something, must_not_alter_existing_files};

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
            mode: spfs::tracking::DiffMode::Changed,
            entries: None,
        }],
        "/spfs",
    );
    assert!(res.is_some())
}
