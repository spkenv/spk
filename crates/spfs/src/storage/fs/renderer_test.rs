use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::{copy_manifest, was_render_completed};
use crate::encoding::Encodable;
use crate::graph::Manifest;
use crate::storage::{fs::FSRepository, ManifestViewer, PayloadStorage, Repository};
use crate::tracking;

fixtures!();

#[rstest]
#[tokio::test]
async fn test_render_manifest(tmpdir: tempdir::TempDir) {
    let mut storage = FSRepository::create(tmpdir.path().join("storage")).unwrap();

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = tracking::compute_manifest(&src_dir).unwrap();

    for node in manifest.walk_abs(&src_dir.to_str().unwrap()) {
        if node.entry.kind.is_blob() {
            let mut data = std::fs::File::open(&node.path.to_path("/")).unwrap();
            storage.write_data(Box::new(&mut data)).unwrap();
        }
    }

    let expected = Manifest::from(&manifest);
    let rendered_path = storage
        .render_manifest(&expected)
        .expect("should successfully rener manfest");
    let actual = Manifest::from(&tracking::compute_manifest(rendered_path).unwrap());
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}

#[rstest]
#[tokio::test]
async fn test_copy_manfest(tmpdir: tempdir::TempDir) {
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    std::os::unix::fs::symlink("file.txt", src_dir.join("dir2.0/file2.txt")).unwrap();
    std::os::unix::fs::symlink(&src_dir, src_dir.join("dir2.0/abssrc")).unwrap();
    std::fs::set_permissions(
        src_dir.join("dir2.0"),
        std::fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    ensure(src_dir.join("file.txt"), "rootdata");
    std::fs::set_permissions(
        src_dir.join("file.txt"),
        std::fs::Permissions::from_mode(0o400),
    )
    .unwrap();

    let expected = Manifest::from(&tracking::compute_manifest(&src_dir).unwrap());

    let dst_dir = tmpdir.path().join("dest");
    copy_manifest(&expected, &src_dir, &dst_dir).expect("failed to copy manifest");

    let actual = Manifest::from(&tracking::compute_manifest(&dst_dir).unwrap());

    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}

#[rstest]
#[tokio::test]
async fn test_render_manifest_with_repo(tmpdir: tempdir::TempDir) {
    let mut tmprepo = FSRepository::create(tmpdir.path().join("repo")).unwrap();
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = Manifest::from(&tmprepo.commit_dir(&src_dir).unwrap());

    let render = tmprepo
        .renders
        .build_digest_path(&manifest.digest().unwrap());
    assert!(!render.exists(), "render should NOT be seen as existing");
    tmprepo.render_manifest(&manifest).unwrap();
    assert!(render.exists(), "render should be seen as existing");
    assert!(was_render_completed(&render));
    let rendered_manifest = Manifest::from(&tracking::compute_manifest(&render).unwrap());
    assert_eq!(
        rendered_manifest.digest().unwrap(),
        manifest.digest().unwrap()
    );
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
