// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
    let mut storage = FSRepository::create(tmpdir.path().join("storage")).await.unwrap();

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = tracking::compute_manifest(&src_dir).await.unwrap();

    for node in manifest.walk_abs(&src_dir.to_str().unwrap()) {
        // TODO: parallelize this process across many files at once
        if node.entry.kind.is_blob() {
            let data = tokio::fs::File::open(&node.path.to_path("/"))
                .await
                .unwrap();
            storage.write_data(Box::pin(data)).await.unwrap();
        }
    }

    let expected = Manifest::from(&manifest);
    let rendered_path = storage
        .render_manifest(&expected)
        .await
        .expect("should successfully rener manfest");
    let actual = Manifest::from(&tracking::compute_manifest(rendered_path).await.unwrap());
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}

#[rstest]
#[tokio::test]
async fn test_render_manifest_with_repo(tmpdir: tempdir::TempDir) {
    let mut tmprepo = FSRepository::create(tmpdir.path().join("repo")).await.unwrap();
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let expected_manifest = tmprepo.commit_dir(&src_dir).await.unwrap();
    let manifest = Manifest::from(&expected_manifest);

    let render = tmprepo
        .renders
        .as_ref()
        .unwrap()
        .build_digest_path(&manifest.digest().unwrap());
    assert!(!render.exists(), "render should NOT be seen as existing");
    tmprepo.render_manifest(&manifest).await.unwrap();
    assert!(render.exists(), "render should be seen as existing");
    assert!(was_render_completed(&render));
    let rendered_manifest = tracking::compute_manifest(&render).await.unwrap();
    let diffs = tracking::compute_diff(&expected_manifest, &rendered_manifest);
    println!("DIFFS:");
    println!("{}", crate::io::format_diffs(diffs.iter()));
    assert_eq!(
        Manifest::from(&expected_manifest).digest().unwrap(),
        Manifest::from(&rendered_manifest).digest().unwrap()
    );
}
