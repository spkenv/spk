use rstest::rstest;

use super::{compute_manifest, EntryKind, Manifest};
use crate::graph;

fixtures!();

#[rstest]
#[tokio::test]
async fn test_compute_manifest_determinism() {
    let first = compute_manifest("./src").unwrap();
    let second = compute_manifest("./src").unwrap();
    assert_eq!(first, second);
}

#[rstest]
#[tokio::test]
async fn test_compute_manifest() {
    let root = std::fs::canonicalize("./src").unwrap();
    let this = file!().to_string().replace("./", "").replace("src/", "");
    let manifest = compute_manifest(root).unwrap();
    assert!(manifest.get_path(&this).is_some());
}

#[rstest]
#[tokio::test]
async fn test_manifest_relative_paths(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    ensure(tmpdir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(tmpdir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(tmpdir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(tmpdir.join("a_file.txt"), "rootdata");

    let manifest = compute_manifest(tmpdir).unwrap();
    assert!(
        manifest.list_dir("/").is_some(),
        "should be able to list root"
    );
    assert!(manifest.get_path("/dir1.0/dir2.0/file.txt").is_some());
    assert!(manifest.get_path("dir1.0/dir2.1/file.txt").is_some());
}
#[rstest]
#[tokio::test]
async fn test_manifest_sorting(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path().join("data");
    ensure(tmpdir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(tmpdir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(tmpdir.join("dir1.0/file.txt"), "thebestdata");
    ensure(tmpdir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(tmpdir.join("a_file.txt"), "rootdata");
    ensure(tmpdir.join("z_file.txt"), "rootdata");

    let manifest = compute_manifest(tmpdir).unwrap();

    let mut actual: Vec<_> = manifest.walk().collect();
    actual.sort();
    let actual: Vec<_> = actual.into_iter().map(|n| n.path).collect();
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
#[rstest]
#[tokio::test]
async fn test_layer_manifests(tmpdir: tempdir::TempDir) {
    let a_dir = tmpdir.path().join("a");
    ensure(a_dir.join("a.txt"), "a");
    ensure(a_dir.join("both.txt"), "a");
    let mut a = compute_manifest(a_dir).unwrap();

    let b_dir = tmpdir.path().join("b");
    ensure(b_dir.join("b.txt"), "b");
    ensure(b_dir.join("both.txt"), "b");
    let b = compute_manifest(b_dir).unwrap();

    let both_dir = tmpdir.path().join("both");
    ensure(both_dir.join("a.txt"), "a");
    ensure(both_dir.join("b.txt"), "b");
    ensure(both_dir.join("both.txt"), "b");
    let both = compute_manifest(both_dir).unwrap();

    a.update(&b);

    assert_eq!(a, both);
    assert_eq!(graph::Manifest::from(&a), graph::Manifest::from(&both));
}
#[rstest]
#[tokio::test]
async fn test_layer_manifests_removal() {
    let mut a = Manifest::default();
    a.mkfile("a_only").unwrap();

    let mut b = Manifest::default();
    let mut node = b.mkfile("a_only").unwrap();
    node.kind = EntryKind::Mask;

    let mut c = Manifest::default();
    c.update(&a);
    assert!(c.get_path("/a_only").unwrap().kind.is_blob());
    c.update(&b);
    assert!(c.get_path("/a_only").unwrap().kind.is_mask());

    compute_manifest("./src").unwrap();
}

fn ensure(path: std::path::PathBuf, data: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("failed to make dirs");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .expect("failed to create file");
    std::io::copy(&mut data.as_bytes(), &mut file).expect("failed to write file data");
}
