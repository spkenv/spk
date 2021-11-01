// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{compute_diff, Diff, DiffMode};
use crate::tracking::{compute_manifest, Manifest};

use crate::fixtures::*;

#[rstest]
fn test_diff_str() {
    let display = format!(
        "{}",
        Diff {
            mode: DiffMode::Added,
            path: "some_path".into(),
            entries: None
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
async fn test_compute_diff_same(tmpdir: tempdir::TempDir) {
    let dir = tmpdir.path();
    std::fs::create_dir_all(dir.join("dir/dir")).unwrap();
    std::fs::write(dir.join("dir/dir/file"), "data").unwrap();
    std::fs::write(dir.join("dir/file"), "more").unwrap();
    std::fs::write(dir.join("file"), "otherdata").unwrap();

    let manifest = compute_manifest(&dir).await.unwrap();
    let diffs = compute_diff(&manifest, &manifest);
    for diff in diffs {
        assert_eq!(diff.mode, DiffMode::Unchanged);
    }
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_added(tmpdir: tempdir::TempDir) {
    let dir = tmpdir.path();
    let a_dir = dir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = dir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(b_dir.join("dir/dir")).unwrap();
    std::fs::write(b_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).await.unwrap();
    let b = compute_manifest(b_dir).await.unwrap();
    let actual = compute_diff(&a, &b);
    let expected = vec![
        Diff {
            mode: DiffMode::Added,
            path: "/dir".into(),
            entries: None,
        },
        Diff {
            mode: DiffMode::Added,
            path: "/dir/dir".into(),
            entries: None,
        },
        Diff {
            mode: DiffMode::Added,
            path: "/dir/dir/file".into(),
            entries: None,
        },
    ];
    assert_eq!(actual, expected);
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_removed(tmpdir: tempdir::TempDir) {
    let dir = tmpdir.path();
    let a_dir = dir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = dir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(a_dir.join("dir/dir")).unwrap();
    std::fs::write(a_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).await.unwrap();
    let b = compute_manifest(b_dir).await.unwrap();
    let actual = compute_diff(&a, &b);
    let expected = vec![
        Diff {
            mode: DiffMode::Removed,
            path: "/dir".into(),
            entries: None,
        },
        Diff {
            mode: DiffMode::Removed,
            path: "/dir/dir".into(),
            entries: None,
        },
        Diff {
            mode: DiffMode::Removed,
            path: "/dir/dir/file".into(),
            entries: None,
        },
    ];
    assert_eq!(actual, expected);
}
