// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;

use super::resolve_stack_to_layers;
use crate::fixtures::*;
use crate::io::DigestFormat;
use crate::prelude::*;
use crate::{encoding, graph, io};

#[rstest]
#[tokio::test]
async fn test_stack_to_layers_dedupe(#[future] tmprepo: TempRepo) {
    let repo = tmprepo.await;
    let layer = graph::Layer::new(encoding::EMPTY_DIGEST.into());
    let platform = graph::Platform::from_digestible([&layer, &layer]).unwrap();
    let mut stack = graph::Stack::from_digestible([&layer]).unwrap();
    stack.push(platform.digest().unwrap());
    repo.write_object(&layer).await.unwrap();
    repo.write_object(&platform).await.unwrap();
    let resolved = resolve_stack_to_layers(&stack, Some(&repo)).await.unwrap();
    assert_eq!(resolved.len(), 1, "should deduplicate layers in resolve");
}

/// Test that if there are too many layers to fit on a single mount
/// that enough layers are merged together so the mount will succeed.
#[rstest]
#[tokio::test]
async fn test_auto_merge_layers(tmpdir: tempfile::TempDir) {
    // A number that is sure to be too many to fit.
    const NUM_LAYERS: usize = 50;
    // This test must use the "local" repository for spfs-render to succeed.
    let config = crate::get_config().expect("get config");
    let fs_repo = config
        .get_opened_local_repository()
        .await
        .expect("open local repository");
    let repo = Arc::new(fs_repo.clone().into());
    let mut layers = Vec::with_capacity(NUM_LAYERS);
    for num in 0..NUM_LAYERS {
        let data_dir = tmpdir.path().join("work").join(format!("dir_{num}"));
        ensure(data_dir.join("file.txt"), &format!("hello world {num}"));
        let manifest = crate::Committer::new(&repo)
            .commit_dir(data_dir.as_path())
            .await
            .unwrap();
        let layer = repo
            .create_layer(&manifest.to_graph_manifest())
            .await
            .unwrap();
        layers.push(layer);
    }

    let storage = crate::runtime::Storage::new(fs_repo.clone()).unwrap();
    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");
    for layer in &layers {
        runtime.push_digest(layer.digest().unwrap());
    }

    let dirs = crate::resolve::resolve_overlay_dirs(
        &config.filesystem.overlayfs_options,
        &mut runtime,
        &fs_repo,
        true,
    )
    .await
    .expect("resolve overlay dirs successfully");

    assert!(
        dirs.len() < layers.len(),
        "some layers were merged {} < {}",
        dirs.len(),
        layers.len()
    );
}

/// Test that if there are too many layers to fit on a single mount
/// and the topmost layer contains an edit of the next top most layer
/// that after merging layers the edit remains in the merged layer
#[rstest]
#[tokio::test]
async fn test_auto_merge_layers_with_edit(tmpdir: tempfile::TempDir) {
    // A number that is sure to be too many to fit.
    const NUM_LAYERS: usize = 40;
    // This test must use the "local" repository for spfs-render to succeed.
    let config = crate::get_config().expect("get config");
    let fs_repo = config
        .get_opened_local_repository()
        .await
        .expect("open local repository");
    let repo = Arc::new(fs_repo.clone().into());

    // Set up the layers with different contents
    let mut layers = Vec::with_capacity(NUM_LAYERS + 1);
    for num in 0..NUM_LAYERS {
        let data_dir = tmpdir.path().join("work").join(format!("dir_{num}"));
        ensure(data_dir.join("file.txt"), &format!("hello world {num}"));
        let manifest = crate::Committer::new(&repo)
            .commit_dir(data_dir.as_path())
            .await
            .unwrap();
        let layer = repo
            .create_layer(&manifest.to_graph_manifest())
            .await
            .unwrap();
        layers.push(layer);
    }

    // Add the top most layer with the edit on the same file that is
    // in the previous layer
    let same_as_prev_layer = NUM_LAYERS - 1;
    let data_dir = tmpdir
        .path()
        .join("work")
        .join(format!("dir_{same_as_prev_layer}"));
    let expected_contents = String::from("hi world");
    ensure(data_dir.join("file.txt"), &expected_contents.to_string());
    let manifest = crate::Committer::new(&repo)
        .commit_dir(data_dir.as_path())
        .await
        .unwrap();
    let layer = repo
        .create_layer(&manifest.to_graph_manifest())
        .await
        .unwrap();
    layers.push(layer);

    let storage = crate::runtime::Storage::new(fs_repo.clone()).unwrap();
    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");
    for layer in &layers {
        runtime.push_digest(layer.digest().unwrap());
    }

    // Test - merging layers doesn't lose the edit
    let dirs = crate::resolve::resolve_overlay_dirs(
        &config.filesystem.overlayfs_options,
        &mut runtime,
        &fs_repo,
        true,
    )
    .await
    .expect("resolve overlay dirs successfully");

    // Check the results
    let d = dirs.last().unwrap();
    for node in d.to_tracking_manifest().walk_abs("/spfs") {
        // There should only be one node/entry, the file.
        let blob = repo.read_blob(node.entry.object).await.unwrap();
        println!(
            " {} {}",
            node.path,
            io::format_digest(node.entry.object, DigestFormat::Full)
                .await
                .unwrap(),
        );

        let (mut payload, _filename) = repo.open_payload(*blob.digest()).await.unwrap();
        let mut writer: Vec<u8> = vec![];
        tokio::io::copy(&mut payload, &mut writer).await.unwrap();
        let contents = String::from_utf8(writer).unwrap();
        assert_eq!(
            contents, expected_contents,
            "top layer's file's edit should have been retained"
        );
    }
}
