use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::was_render_completed;
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
async fn test_render_manifest_with_repo(tmpdir: tempdir::TempDir) {
    let mut tmprepo = FSRepository::create(tmpdir.path().join("repo")).unwrap();
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let expected_manifest = tmprepo.commit_dir(&src_dir).unwrap();
    let manifest = Manifest::from(&expected_manifest);

    let render = tmprepo
        .renders
        .as_ref()
        .unwrap()
        .build_digest_path(&manifest.digest().unwrap());
    assert!(!render.exists(), "render should NOT be seen as existing");
    tmprepo.render_manifest(&manifest).unwrap();
    assert!(render.exists(), "render should be seen as existing");
    assert!(was_render_completed(&render));
    let rendered_manifest = tracking::compute_manifest(&render).unwrap();
    let diffs = tracking::compute_diff(&expected_manifest, &rendered_manifest);
    println!("DIFFS:");
    println!("{}", crate::io::format_diffs(diffs.into_iter()));
    assert_eq!(
        Manifest::from(&expected_manifest).digest().unwrap(),
        Manifest::from(&rendered_manifest).digest().unwrap()
    );
}
