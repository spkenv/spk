// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Tests for the transition away from writing legacy blob objects.
//!
//! See [`crate::config::Storage::write_legacy_blob_objects`].

use rstest::rstest;

use crate::check::{CheckSummary, Checker};
use crate::config::Config;
use crate::fixtures::*;
use crate::graph::{self, FoundDigest};
use crate::prelude::*;
use crate::{Syncer, encoding};

/// Restores the original config when dropped, even if the test panics.
struct ConfigGuard(std::sync::Arc<Config>);

impl ConfigGuard {
    fn new() -> Self {
        Self(Config::current().unwrap())
    }
}

impl Drop for ConfigGuard {
    fn drop(&mut self) {
        (*self.0).clone().make_current().unwrap();
    }
}

fn set_write_legacy_blob_objects(enabled: bool) {
    let mut config = (*Config::current().unwrap()).clone();
    config.storage.write_legacy_blob_objects = enabled;
    config.make_current().unwrap();
}

/// A partial digest that matches only the given digest, in practice
fn partial(digest: &encoding::Digest) -> encoding::PartialDigest {
    encoding::PartialDigest::from(&digest.as_bytes()[..10])
}

#[rstest]
#[case::fs_enabled(tmprepo("fs"), true)]
#[case::fs_disabled(tmprepo("fs"), false)]
#[case::tar_enabled(tmprepo("tar"), true)]
#[case::tar_disabled(tmprepo("tar"), false)]
#[cfg_attr(feature = "server", case::rpc_enabled(tmprepo("rpc"), true))]
#[cfg_attr(feature = "server", case::rpc_disabled(tmprepo("rpc"), false))]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_commit_payload_legacy_blob(
    #[case]
    #[future]
    tmprepo: TempRepo,
    #[case] enabled: bool,
) {
    let tmprepo = tmprepo.await;
    let _config = ConfigGuard::new();
    init_logging();
    set_write_legacy_blob_objects(enabled);

    let digest = tmprepo
        .commit_payload(Box::pin(b"some payload data".as_slice()))
        .await
        .unwrap();

    assert!(tmprepo.has_payload(digest).await);
    assert_eq!(
        tmprepo.has_object(digest).await,
        enabled,
        "a blob object should exist only if enabled"
    );
    if enabled {
        let obj = tmprepo.read_object(digest).await.unwrap();
        let graph::object::Enum::Blob(blob) = obj.into_enum() else {
            panic!("expected a blob object");
        };
        assert_eq!(blob.size(), b"some payload data".len() as u64);
    }

    // The payload must be usable either way
    assert_eq!(
        tmprepo.payload_size(digest).await.unwrap(),
        b"some payload data".len() as u64
    );
    assert_eq!(
        tmprepo
            .resolve_full_digest(&partial(&digest))
            .await
            .unwrap(),
        FoundDigest::Payload(digest),
        "a payload and its legacy blob should resolve to the payload, not be ambiguous"
    );
    assert_eq!(
        tmprepo
            .resolve_full_digest(&encoding::PartialDigest::from(digest.as_bytes()))
            .await
            .unwrap(),
        FoundDigest::Payload(digest),
    );
}

/// Older clients will continue to write blob objects after the switch is
/// flipped, and that must not fail.
#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_write_blob_object_ignored_when_disabled(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let _config = ConfigGuard::new();
    init_logging();
    set_write_legacy_blob_objects(false);

    let (digest, size) = tmprepo
        .write_data(Box::pin(b"older client data".as_slice()))
        .await
        .unwrap();
    tmprepo
        .write_object(&graph::Blob::new(digest, size))
        .await
        .expect("writing a blob object should be accepted");
    assert!(
        !tmprepo.has_object(digest).await,
        "the blob object should have been discarded"
    );
}

/// Repositories that contain legacy blob objects must still check cleanly.
#[rstest]
#[case::fs(tmprepo("fs"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_check_with_legacy_blob_objects(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let _config = ConfigGuard::new();
    init_logging();
    set_write_legacy_blob_objects(true);

    let digest = tmprepo
        .commit_payload(Box::pin(b"annotation data".as_slice()))
        .await
        .unwrap();
    let layer = graph::Layer::new_with_annotation(
        "test_annotation",
        graph::AnnotationValue::Blob(digest.into()),
    );
    tmprepo.write_object(&layer).await.unwrap();
    assert!(
        tmprepo.has_object(digest).await,
        "test requires a blob object"
    );

    let results = Checker::new(&tmprepo.repo())
        .check_all_objects()
        .await
        .expect("check should succeed with legacy blob objects present");
    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    assert_eq!(summary.valid_objects, 1, "only the layer is counted");
    assert_eq!(summary.valid_payloads, 1, "the payload is checked once");
    assert!(summary.missing_objects.is_empty());
    assert!(summary.missing_payloads.is_empty());

    // Checking the payload digest directly works too
    let result = Checker::new(&tmprepo.repo())
        .check_partial_digest(partial(&digest))
        .await
        .unwrap();
    assert!(result.summary().missing_payloads.is_empty());
    assert_eq!(result.summary().valid_payloads, 1);
}

#[rstest]
#[case::enabled(true)]
#[case::disabled(false)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_sync_legacy_blob_objects(#[case] enabled: bool, tmpdir: tempfile::TempDir) {
    let repo_a = tmprepo("fs").await;
    let repo_b = tmprepo("fs").await;
    let _config = ConfigGuard::new();
    init_logging();
    set_write_legacy_blob_objects(enabled);

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    let manifest = crate::Committer::new(&repo_a)
        .commit_dir(src_dir.as_path())
        .await
        .unwrap()
        .to_graph_manifest();
    let annotation = repo_a
        .commit_payload(Box::pin(b"annotation data".as_slice()))
        .await
        .unwrap();
    let layer = graph::Layer::new_with_manifest_and_annotation(
        manifest.digest().unwrap(),
        "key",
        graph::AnnotationValue::Blob(annotation.into()),
    );
    repo_a.write_object(&layer).await.unwrap();

    Syncer::new(&repo_a.repo(), &repo_b.repo())
        .sync_object_digest(layer.digest().unwrap())
        .await
        .unwrap();

    let payloads: Vec<_> = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .map(|e| *e.object())
        .chain(std::iter::once(annotation))
        .collect();
    assert_eq!(payloads.len(), 3);
    for digest in payloads {
        assert!(repo_b.has_payload(digest).await);
        assert_eq!(
            repo_b.has_object(digest).await,
            enabled,
            "a blob object should be synced only if enabled"
        );
    }
}

/// While blob objects are being written, syncing must restore any that
/// are missing from the destination even when the payload already exists.
#[rstest]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_sync_restores_missing_legacy_blob_object() {
    let repo_a = tmprepo("fs").await;
    let repo_b = tmprepo("fs").await;
    let _config = ConfigGuard::new();
    init_logging();
    set_write_legacy_blob_objects(false);
    let digest = repo_b
        .commit_payload(Box::pin(b"payload".as_slice()))
        .await
        .unwrap();
    set_write_legacy_blob_objects(true);
    repo_a
        .commit_payload(Box::pin(b"payload".as_slice()))
        .await
        .unwrap();
    assert!(!repo_b.has_object(digest).await);

    Syncer::new(&repo_a.repo(), &repo_b.repo())
        .sync_env(crate::tracking::EnvSpec::parse(digest.to_string()).unwrap())
        .await
        .unwrap();
    assert!(repo_b.has_object(digest).await);
}
