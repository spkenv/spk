// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePath;
use rstest::rstest;

use super::{compute_diff, Diff, DiffMode};
use crate::tracking::{compute_manifest, Manifest};

use crate::fixtures::*;

#[rstest]
fn test_diff_str() {
    let display = format!(
        "{}",
        Diff {
            mode: DiffMode::Added(Default::default()),
            path: "some_path".into(),
        }
    );
    assert_eq!(&display, "+ some_path");
}

#[rstest]
fn test_compute_diff_empty() {
    let a = Manifest::default();
    let b = Manifest::default();

    assert_eq!(compute_diff(&a, &b), Vec::new());
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_same(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    std::fs::create_dir_all(dir.join("dir/dir")).unwrap();
    std::fs::write(dir.join("dir/dir/file"), "data").unwrap();
    std::fs::write(dir.join("dir/file"), "more").unwrap();
    std::fs::write(dir.join("file"), "otherdata").unwrap();

    let manifest = compute_manifest(&dir).await.unwrap();
    let diffs = compute_diff(&manifest, &manifest);
    for diff in diffs {
        assert!(diff.mode.is_unchanged());
    }
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_added(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let a_dir = dir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = dir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(b_dir.join("dir/dir")).unwrap();
    std::fs::write(b_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).await.unwrap();
    let b = compute_manifest(b_dir).await.unwrap();
    let mut actual = compute_diff(&a, &b).into_iter();

    let first = actual.next().unwrap();
    let second = actual.next().unwrap();
    let third = actual.next().unwrap();
    assert!(actual.next().is_none());

    assert!(matches!(first.mode, DiffMode::Added(..)));
    assert_eq!(&first.path, &RelativePath::new("/dir"));
    assert!(matches!(second.mode, DiffMode::Added(..)));
    assert_eq!(&second.path, &RelativePath::new("/dir/dir"));
    assert!(matches!(third.mode, DiffMode::Added(..)));
    assert_eq!(&third.path, &RelativePath::new("/dir/dir/file"));
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_removed(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let a_dir = dir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = dir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(a_dir.join("dir/dir")).unwrap();
    std::fs::write(a_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).await.unwrap();
    let b = compute_manifest(b_dir).await.unwrap();
    let mut actual = compute_diff(&a, &b).into_iter();
    let first = actual.next().unwrap();
    let second = actual.next().unwrap();
    let third = actual.next().unwrap();
    assert!(actual.next().is_none());

    assert!(matches!(first.mode, DiffMode::Removed(..)));
    assert_eq!(&first.path, &RelativePath::new("/dir"));
    assert!(matches!(second.mode, DiffMode::Removed(..)));
    assert_eq!(&second.path, &RelativePath::new("/dir/dir"));
    assert!(matches!(third.mode, DiffMode::Removed(..)));
    assert_eq!(&third.path, &RelativePath::new("/dir/dir/file"));
}
