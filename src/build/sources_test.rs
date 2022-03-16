// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use super::validate_source_changeset;

#[test]
fn test_validate_sources_changeset_nothing() {
    let res = validate_source_changeset(vec![], "/spfs");
    assert!(res.is_err());
}

#[test]
fn test_validate_sources_changeset_not_in_dir() {
    let res = validate_source_changeset(
        vec![spfs::tracking::Diff {
            path: "/file.txt".into(),
            mode: spfs::tracking::DiffMode::Changed(Default::default(), Default::default()),
        }],
        "/some/dir",
    );
    assert!(res.is_err());
}

#[test]
fn test_validate_sources_changeset_ok() {
    let res = validate_source_changeset(
        vec![spfs::tracking::Diff {
            path: "/some/dir/file.txt".into(),
            mode: spfs::tracking::DiffMode::Added(Default::default()),
        }],
        "/some/dir",
    );
    assert!(res.is_ok());
}
