use rstest::rstest;

use super::{Entry, Manifest};
use crate::{
    encoding::{self, Encodable},
    tracking,
};

fixtures!();

#[rstest]
#[tokio::test]
async fn test_entry_blobs_compare_name() {
    let a = Entry {
        name: "a".to_string(),
        kind: tracking::EntryKind::Blob,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    let b = Entry {
        name: "b".to_string(),
        kind: tracking::EntryKind::Blob,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    assert!(a < b);
    assert!(b > a);
}

#[rstest]
#[tokio::test]
async fn test_entry_trees_compare_name() {
    let a = Entry {
        name: "a".to_string(),
        kind: tracking::EntryKind::Tree,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    let b = Entry {
        name: "b".to_string(),
        kind: tracking::EntryKind::Tree,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    assert!(a < b);
    assert!(b > a);
}

#[rstest]
#[tokio::test]
async fn test_entry_compare_kind() {
    let blob = Entry {
        name: "a".to_string(),
        kind: tracking::EntryKind::Blob,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    let tree = Entry {
        name: "b".to_string(),
        kind: tracking::EntryKind::Tree,
        mode: 0,
        object: encoding::EMPTY_DIGEST.into(),
        size: 0,
    };
    assert!(tree > blob);
    assert!(blob < tree);
}

#[rstest]
#[tokio::test]
async fn test_entry_compare() {
    let root_file = Entry {
        name: "file".to_string(),
        kind: tracking::EntryKind::Blob,
        mode: 0,
        object: encoding::NULL_DIGEST.into(),
        size: 0,
    };
    let root_dir = Entry {
        name: "xdir".to_string(),
        kind: tracking::EntryKind::Tree,
        mode: 0,
        object: encoding::NULL_DIGEST.into(),
        size: 0,
    };
    assert!(root_dir > root_file);
}

#[rstest]
#[tokio::test]
async fn test_manifest_sorting(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();
    let dir = tmpdir.path().join("data");
    ensure(dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(dir.join("dir1.0/file.txt"), "thebestdata");
    ensure(dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(dir.join("a_file.txt"), "rootdata");
    ensure(dir.join("z_file.txt"), "rootdata");

    let tracking_manifest = crate::tracking::compute_manifest(dir).unwrap();
    let manifest = Manifest::from(&tracking_manifest);

    assert_eq!(
        manifest.digest().unwrap(),
        tracking_manifest.root().object,
        "tracking and graph manifests should share a digest"
    );

    let actual = manifest.iter_entries();
    let actual: Vec<_> = actual.into_iter().map(|n| n.name.clone()).collect();
    let expected = vec![
        "/dir1.0",
        "/dir1.0/dir2.0",
        "/dir1.0/dir2.0/file.txt",
        "/dir1.0/dir2.1",
        "/dir1.0/dir2.1/file.txt",
        "/dir1.0/file.txt",
        "/dir2.0",
        "/dir2.0/file.txt",
        "/a_file.txt",
        "/z_file.txt",
    ];
    assert_eq!(actual, expected);
}
