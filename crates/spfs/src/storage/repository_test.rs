// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::iter::FromIterator;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use rstest::rstest;

use super::Ref;
use crate::graph::Manifest;
use crate::storage::{fs, prelude::*};
use crate::{encoding::Encodable, tracking::TagSpec};

use crate::fixtures::*;

#[rstest(
    tmprepo,
    case(tmprepo("fs")),
    case(tmprepo("tar")),
    case(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_find_aliases(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    tmprepo
        .find_aliases("not-existent")
        .await
        .expect_err("should error when ref is not found");

    let manifest = crate::commit_dir(tmprepo.repo(), "src/storage")
        .await
        .unwrap();
    let layer = tmprepo
        .create_layer(&Manifest::from(&manifest))
        .await
        .unwrap();
    let test_tag = TagSpec::parse("test-tag").unwrap();
    tmprepo
        .push_tag(&test_tag, &layer.digest().unwrap())
        .await
        .unwrap();

    let actual = tmprepo
        .find_aliases(layer.digest().unwrap().to_string().as_ref())
        .await
        .unwrap();
    let expected = HashSet::from_iter(vec![Ref::TagSpec(test_tag)]);
    assert_eq!(actual, expected);
    let actual = tmprepo.find_aliases("test-tag").await.unwrap();
    let expected = HashSet::from_iter(vec![Ref::Digest(layer.digest().unwrap())]);
    assert_eq!(actual, expected);
}

#[rstest]
#[tokio::test]
async fn test_commit_mode_fs(tmpdir: tempdir::TempDir) {
    init_logging();
    let dir = tmpdir.path();
    let tmprepo = Arc::new(
        fs::FSRepository::create(dir.join("repo"))
            .await
            .unwrap()
            .into(),
    );
    let datafile_path = "dir1.0/dir2.0/file.txt";
    let symlink_path = "dir1.0/dir2.0/file2.txt";

    let src_dir = dir.join("source");
    std::fs::create_dir_all(src_dir.join("dir1.0/dir2.0")).unwrap();
    let link_dest = src_dir.join(datafile_path);
    std::fs::write(&link_dest, "somedata").unwrap();
    std::os::unix::fs::symlink(&link_dest, &src_dir.join(symlink_path)).unwrap();
    std::fs::set_permissions(&link_dest, std::fs::Permissions::from_mode(0o444)).unwrap();

    let manifest = crate::commit_dir(Arc::clone(&tmprepo), &src_dir)
        .await
        .expect("failed to commit dir");

    // Safety: tmprepo was created as an FSRepository
    let tmprepo = match unsafe { &*Arc::into_raw(tmprepo) } {
        RepositoryHandle::FS(fs) => fs,
        _ => panic!("Unexpected tmprepo type!"),
    };

    let rendered_dir = tmprepo
        .render_manifest(&Manifest::from(&manifest))
        .await
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

#[rstest(
    tmprepo,
    case(tmprepo("fs")),
    case(tmprepo("tar")),
    case(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_commit_broken_link(#[future] tmprepo: TempRepo, tmpdir: tempdir::TempDir) {
    let tmprepo = tmprepo.await;
    let src_dir = tmpdir.path().join("source");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::os::unix::fs::symlink(
        std::path::Path::new("nonexistent"),
        src_dir.join("broken-link"),
    )
    .unwrap();

    let manifest = crate::commit_dir(tmprepo.repo(), &src_dir).await.unwrap();
    assert!(manifest.get_path("broken-link").is_some());
}

#[rstest(
    tmprepo,
    case(tmprepo("fs")),
    case(tmprepo("tar")),
    case(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_commit_dir(#[future] tmprepo: TempRepo, tmpdir: tempdir::TempDir) {
    let tmprepo = tmprepo.await;
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = Manifest::from(&crate::commit_dir(tmprepo.repo(), &src_dir).await.unwrap());
    let manifest2 = Manifest::from(&crate::commit_dir(tmprepo.repo(), &src_dir).await.unwrap());
    assert_eq!(manifest, manifest2);
}
