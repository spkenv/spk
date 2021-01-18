use rstest::{fixture, rstest};

use super::{compute_diff, Diff, DiffMode};
use crate::tracking::{compute_manifest, Manifest};

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-storage-").expect("failed to create dir for test")
}

#[rstest]
#[tokio::test]
async fn test_diff_str() {
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
#[tokio::test]
async fn test_compute_diff_empty() {
    let a = Manifest::default();
    let b = Manifest::default();

    assert_eq!(compute_diff(&a, &b), Vec::new());
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_same(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    std::fs::create_dir_all(tmpdir.join("dir/dir")).unwrap();
    std::fs::write(tmpdir.join("dir/dir/file"), "data").unwrap();
    std::fs::write(tmpdir.join("dir/file"), "more").unwrap();
    std::fs::write(tmpdir.join("file"), "otherdata").unwrap();

    let manifest = compute_manifest(&tmpdir).unwrap();
    let diffs = compute_diff(&manifest, &manifest);
    for diff in diffs {
        assert_eq!(diff.mode, DiffMode::Unchanged);
    }
}

#[rstest]
#[tokio::test]
async fn test_compute_diff_added(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let a_dir = tmpdir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = tmpdir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(b_dir.join("dir/dir")).unwrap();
    std::fs::write(b_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).unwrap();
    let b = compute_manifest(b_dir).unwrap();
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
    let tmpdir = tmpdir.path();
    let a_dir = tmpdir.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let b_dir = tmpdir.join("b");
    std::fs::create_dir_all(&b_dir).unwrap();
    std::fs::create_dir_all(a_dir.join("dir/dir")).unwrap();
    std::fs::write(a_dir.join("dir/dir/file"), "data").unwrap();

    let a = compute_manifest(a_dir).unwrap();
    let b = compute_manifest(b_dir).unwrap();
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
