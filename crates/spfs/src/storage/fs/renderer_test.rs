// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::rstest;

use super::was_render_completed;
use crate::encoding::Encodable;
use crate::fixtures::*;
use crate::graph::Manifest;
use crate::storage::fs::FSRepository;
use crate::storage::{Repository, RepositoryHandle};
use crate::tracking;

#[rstest]
#[tokio::test]
async fn test_render_manifest(tmpdir: tempfile::TempDir) {
    let storage = FSRepository::create(tmpdir.path().join("storage"))
        .await
        .unwrap();

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let manifest = tracking::compute_manifest(&src_dir).await.unwrap();

    for node in manifest.walk_abs(src_dir.to_str().unwrap()) {
        if node.entry.kind.is_blob() {
            let data = tokio::fs::File::open(&node.path.to_path("/"))
                .await
                .unwrap();
            storage
                .commit_blob(Box::pin(tokio::io::BufReader::new(data)))
                .await
                .unwrap();
        }
    }

    let expected = Manifest::from(&manifest);
    let rendered_path = crate::storage::fs::Renderer::new(&storage)
        .render_manifest(&expected, None)
        .await
        .expect("should successfully render manifest");
    let actual = Manifest::from(&tracking::compute_manifest(rendered_path).await.unwrap());
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}

#[rstest]
#[tokio::test]
async fn test_render_manifest_with_repo(tmpdir: tempfile::TempDir) {
    let tmprepo = Arc::new(
        FSRepository::create(tmpdir.path().join("repo"))
            .await
            .unwrap()
            .into(),
    );
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
    ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
    ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
    ensure(src_dir.join("file.txt"), "rootdata");

    let expected_manifest = crate::commit_dir(Arc::clone(&tmprepo), &src_dir)
        .await
        .unwrap();
    let manifest = Manifest::from(&expected_manifest);

    // Safety: tmprepo was created as an FSRepository
    let tmprepo = match unsafe { &*Arc::into_raw(tmprepo) } {
        RepositoryHandle::FS(fs) => fs,
        _ => panic!("Unexpected tmprepo type!"),
    };

    let render = tmprepo
        .renders
        .as_ref()
        .unwrap()
        .renders
        .build_digest_path(&manifest.digest().unwrap());
    assert!(!render.exists(), "render should NOT be seen as existing");
    super::Renderer::new(tmprepo)
        .render_manifest(&manifest, None)
        .await
        .unwrap();
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
