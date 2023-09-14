// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::iter::FromIterator;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;

use rstest::rstest;

use super::Ref;
use crate::encoding::Encodable;
use crate::fixtures::*;
use crate::graph::Manifest;
use crate::storage::fs;
use crate::storage::prelude::*;
use crate::tracking::TagSpec;

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_find_aliases(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;
    tmprepo
        .find_aliases("not-existent")
        .await
        .expect_err("should error when ref is not found");

    let manifest = crate::Committer::new(&tmprepo)
        .commit_dir("src/storage")
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
async fn test_commit_mode_fs(tmpdir: tempfile::TempDir) {
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
    std::os::unix::fs::symlink(&link_dest, src_dir.join(symlink_path)).unwrap();
    std::fs::set_permissions(&link_dest, std::fs::Permissions::from_mode(0o444)).unwrap();

    let manifest = crate::Committer::new(&tmprepo)
        .commit_dir(&src_dir)
        .await
        .expect("failed to commit dir");

    // Safety: tmprepo was created as an FSRepository
    let tmprepo = match &*tmprepo {
        RepositoryHandle::FS(fs) => fs.opened().await.unwrap(),
        _ => panic!("Unexpected tmprepo type!"),
    };

    let rendered_dir = fs::Renderer::new(&*tmprepo)
        .render_manifest(&Manifest::from(&manifest), None)
        .await
        .expect("failed to render manifest");
    let rendered_symlink = rendered_dir.join(symlink_path);
    let rendered_mode = rendered_symlink.symlink_metadata().unwrap().mode();
    assert!(unix_mode::is_symlink(rendered_mode), "should be a symlink");

    let symlink_entry = manifest
        .get_path(symlink_path)
        .expect("symlink not in manifest");
    let symlink_blob = tmprepo.payloads.build_digest_path(&symlink_entry.object);
    let blob_mode = symlink_blob.symlink_metadata().unwrap().mode();
    assert!(
        !unix_mode::is_symlink(blob_mode),
        "stored blob should not be a symlink"
    )
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_commit_broken_link(
    #[case]
    #[future]
    tmprepo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    let tmprepo = tmprepo.await;
    let src_dir = tmpdir.path().join("source");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::os::unix::fs::symlink(
        std::path::Path::new("nonexistent"),
        src_dir.join("broken-link"),
    )
    .unwrap();

    let manifest = crate::Committer::new(&tmprepo)
        .commit_dir(&src_dir)
        .await
        .unwrap();
    assert!(manifest.get_path("broken-link").is_some());
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_commit_dir(
    #[case]
    #[future]
    tmprepo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    let tmprepo = tmprepo.await;
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = Manifest::from(
        &crate::Committer::new(&tmprepo)
            .commit_dir(&src_dir)
            .await
            .unwrap(),
    );
    let manifest2 = Manifest::from(
        &crate::Committer::new(&tmprepo)
            .commit_dir(&src_dir)
            .await
            .unwrap(),
    );
    assert_eq!(manifest, manifest2);
}
