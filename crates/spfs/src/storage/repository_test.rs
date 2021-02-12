use std::collections::HashSet;
use std::iter::FromIterator;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::{Ref, Repository};
use crate::graph::Manifest;
use crate::storage::{fs, prelude::*};
use crate::{encoding::Encodable, tracking::TagSpec};

fixtures!();

#[rstest(tmprepo, case(tmprepo("fs")), case(tmprepo("tar")))]
#[tokio::test]
async fn test_find_aliases(mut tmprepo: super::super::RepositoryHandle) {
    tmprepo
        .find_aliases("not-existant")
        .expect_err("should error when ref is not found");

    let manifest = tmprepo.commit_dir("src/storage".as_ref()).unwrap();
    let layer = tmprepo.create_layer(&Manifest::from(&manifest)).unwrap();
    let test_tag = TagSpec::parse("test-tag").unwrap();
    tmprepo
        .push_tag(&test_tag, &layer.digest().unwrap())
        .unwrap();

    let actual = tmprepo
        .find_aliases(layer.digest().unwrap().to_string().as_ref())
        .unwrap();
    let expected = HashSet::from_iter(vec![Ref::TagSpec(test_tag)]);
    assert_eq!(actual, expected);
    let actual = tmprepo.find_aliases("test-tag").unwrap();
    let expected = HashSet::from_iter(vec![Ref::Digest(layer.digest().unwrap())]);
    assert_eq!(actual, expected);
}

#[rstest]
#[tokio::test]
async fn test_commit_mode_fs(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let mut tmprepo = fs::FSRepository::create(tmpdir.join("repo")).unwrap();
    let datafile_path = "dir1.0/dir2.0/file.txt";
    let symlink_path = "dir1.0/dir2.0/file2.txt";

    let src_dir = tmpdir.join("source");
    std::fs::create_dir_all(tmpdir.join("dir1.0/dir2.0")).unwrap();
    let link_dest = src_dir.join(datafile_path);
    std::fs::write(&link_dest, "somedata").unwrap();
    std::os::unix::fs::symlink(&src_dir.join(symlink_path), &link_dest).unwrap();
    std::fs::set_permissions(&link_dest, std::fs::Permissions::from_mode(0o444)).unwrap();

    let manifest = tmprepo.commit_dir(&src_dir).expect("failed to commit dir");
    let rendered_dir = tmprepo
        .render_manifest(&Manifest::from(&manifest))
        .expect("failed to render manifest");
    let rendered_symlink = rendered_dir.join(symlink_path);
    assert!(
        rendered_symlink.symlink_metadata().unwrap().mode() & libc::S_IFLNK > 0,
        "should be a symlink"
    );

    let symlink_entry = manifest
        .get_path(symlink_path)
        .expect("symlink not in manifest");
    let symlink_blob = tmprepo.payloads.build_digest_path(&symlink_entry.object);
    assert!(
        symlink_blob.symlink_metadata().unwrap().mode() & libc::S_IFLNK == 0,
        "stored blob should not be a symlink"
    )
}

#[rstest(tmprepo, case(tmprepo("fs")), case(tmprepo("tar")))]
#[tokio::test]
async fn test_commit_broken_link(
    tmpdir: tempdir::TempDir,
    mut tmprepo: super::super::RepositoryHandle,
) {
    let src_dir = tmpdir.path().join("source");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::os::unix::fs::symlink(
        std::path::Path::new("nonexistant"),
        src_dir.join("broken-link"),
    )
    .unwrap();

    let manifest = tmprepo.commit_dir(&src_dir).unwrap();
    assert!(manifest.get_path("broken-link").is_some());
}

#[rstest(tmprepo, case::fs(tmprepo("fs")), case::fs(tmprepo("tar")))]
#[tokio::test]
async fn test_commit_dir(tmpdir: tempdir::TempDir, mut tmprepo: super::super::RepositoryHandle) {
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = Manifest::from(&tmprepo.commit_dir(&src_dir).unwrap());
    let manifest2 = Manifest::from(&tmprepo.commit_dir(&src_dir).unwrap());
    assert_eq!(manifest, manifest2);
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
