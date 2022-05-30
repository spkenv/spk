use std::sync::Arc;

// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::resolve_stack_to_layers;
use crate::{encoding, graph, prelude::*};

use crate::fixtures::*;

#[rstest]
#[tokio::test]
async fn test_stack_to_layers_dedupe(#[future] tmprepo: TempRepo) {
    let repo = tmprepo.await;
    let layer = graph::Layer::new(encoding::EMPTY_DIGEST.into());
    let platform = graph::Platform::new(vec![layer.clone(), layer.clone()].into_iter()).unwrap();
    let stack = vec![layer.digest().unwrap(), platform.digest().unwrap()];
    repo.write_object(&graph::Object::Layer(layer))
        .await
        .unwrap();
    repo.write_object(&graph::Object::Platform(platform))
        .await
        .unwrap();
    let resolved = resolve_stack_to_layers(stack.into_iter(), Some(&repo))
        .await
        .unwrap();
    assert_eq!(resolved.len(), 1, "should deduplicate layers in resolve");
}

/// Test that if there are too many layers to fit on a single mount
/// that enough layers are merged together so the mount will succeed.
#[rstest]
#[tokio::test]
async fn test_auto_merge_layers(tmpdir: tempdir::TempDir) {
    // A number that is sure to be too many to fit.
    const NUM_LAYERS: usize = 50;
    // This test must use the "local" repository for spfs-render to succeed.
    let config = crate::get_config().expect("get config");
    let repo = Arc::new(
        config
            .get_local_repository()
            .await
            .expect("get local repository")
            .into(),
    );
    let mut layers = Vec::with_capacity(NUM_LAYERS);
    for num in 0..NUM_LAYERS {
        let data_dir = tmpdir.path().join("work").join(format!("dir_{num}"));
        ensure(data_dir.join("file.txt"), &format!("hello world {num}"));
        let manifest = crate::commit_dir(Arc::clone(&repo), data_dir.as_path())
            .await
            .unwrap();
        let layer = repo
            .create_layer(&graph::Manifest::from(&manifest))
            .await
            .unwrap();
        layers.push(layer);
    }

    let storage = crate::runtime::Storage::new(Arc::clone(&repo));
    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");
    for layer in &layers {
        runtime.push_digest(&layer.digest().unwrap());
    }

    let dirs = crate::resolve::resolve_overlay_dirs(
        &runtime,
        &*repo,
        &crate::env::OverlayMountOptions::default(),
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
