// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
fn test_find_aliases(tmprepo: TempRepo) {
    let (_td, mut tmprepo) = tmprepo;
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
fn test_commit_mode_fs(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();
    let dir = tmpdir.path();
    let mut tmprepo = fs::FSRepository::create(dir.join("repo")).unwrap();
    let datafile_path = "dir1.0/dir2.0/file.txt";
    let symlink_path = "dir1.0/dir2.0/file2.txt";

    let src_dir = dir.join("source");
    std::fs::create_dir_all(src_dir.join("dir1.0/dir2.0")).unwrap();
    let link_dest = src_dir.join(datafile_path);
    std::fs::write(&link_dest, "somedata").unwrap();
    std::os::unix::fs::symlink(&link_dest, &src_dir.join(symlink_path)).unwrap();
    std::fs::set_permissions(&link_dest, std::fs::Permissions::from_mode(0o444)).unwrap();

    let manifest = tmprepo.commit_dir(&src_dir).expect("failed to commit dir");
    let rendered_dir = tmprepo
        .render_manifest(&Manifest::from(&manifest))
        .expect("failed to render manifest");
    let rendered_symlink = rendered_dir.join(symlink_path);
    let rendered_mode = rendered_symlink.symlink_metadata().unwrap().mode();
    assert!(
        (libc::S_IFMT & rendered_mode) == libc::S_IFLNK,
        "should be a symlink"
    );

    let symlink_entry = manifest
        .get_path(symlink_path)
        .expect("symlink not in manifest");
    let symlink_blob = tmprepo.payloads.build_digest_path(&symlink_entry.object);
    let blob_mode = symlink_blob.symlink_metadata().unwrap().mode();
    assert!(
        (libc::S_IFMT & blob_mode) != libc::S_IFLNK,
        "stored blob should not be a symlink"
    )
}

#[rstest(tmprepo, case(tmprepo("fs")), case(tmprepo("tar")))]
fn test_commit_broken_link(tmprepo: TempRepo) {
    let (tmpdir, mut tmprepo) = tmprepo;
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
fn test_commit_dir(tmprepo: TempRepo) {
    let (tmpdir, mut tmprepo) = tmprepo;
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = Manifest::from(&tmprepo.commit_dir(&src_dir).unwrap());
    let manifest2 = Manifest::from(&tmprepo.commit_dir(&src_dir).unwrap());
    assert_eq!(manifest, manifest2);
}
