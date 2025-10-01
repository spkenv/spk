// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Mutex;

use rstest::rstest;
use spfs_encoding::prelude::*;

use super::{CheckSummary, Checker};
use crate::check::CheckReporter;
use crate::fixtures::*;
use crate::graph::{Database, DatabaseExt};
use crate::storage::{PayloadStorage, RepositoryExt};

#[rstest]
#[tokio::test]
async fn test_check_missing_payload(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;

    let manifest = generate_tree(&tmprepo).await.to_graph_manifest();
    let file = manifest
        .iter_entries()
        .find(|entry| entry.is_regular_file())
        .expect("at least one regular file");

    tracing::info!(digest=%file.object(), "remove payload");
    tmprepo
        .remove_payload(*file.object())
        .await
        .expect("failed to remove payload");

    let total_blobs = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .count();
    let total_objects = total_blobs + 1; //the manifest

    let results = Checker::new(&tmprepo.repo())
        .check_all_objects()
        .await
        .unwrap();

    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    tracing::info!("{summary:#?}");
    assert_eq!(
        summary.checked_objects, total_objects,
        "expected all items to be visited"
    );
    assert_eq!(
        summary.checked_payloads,
        total_blobs - 1,
        "expected all payloads to be visited except missing one"
    );
    assert!(
        summary.missing_payloads.contains(file.object()),
        "should find one missing payload"
    );
    assert_eq!(
        summary.missing_objects.len(),
        0,
        "should see no missing objects"
    );
}

#[rstest]
#[tokio::test]
async fn test_check_missing_object(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;

    let manifest = generate_tree(&tmprepo).await.to_graph_manifest();
    let file = manifest
        .iter_entries()
        .find(|entry| entry.is_regular_file())
        .expect("at least one regular file");

    tracing::info!(digest=%file.object(), "remove object");
    tmprepo
        .remove_object(*file.object())
        .await
        .expect("failed to remove object");

    let total_blobs = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .count();
    let total_objects = total_blobs + 1; // the manifest

    let results = Checker::new(&tmprepo.repo())
        .check_all_objects()
        .await
        .unwrap();

    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    tracing::info!("{summary:#?}");
    assert_eq!(
        summary.checked_objects,
        total_objects - 1,
        "expected all items to be visited except missing one"
    );
    assert_eq!(
        summary.checked_payloads,
        total_blobs - 1,
        "one payload should not be seen because of missing object"
    );
    assert!(
        summary.missing_objects.contains(file.object()),
        "should find one missing object"
    );
    assert!(
        summary.missing_payloads.is_empty(),
        "should see no missing payloads"
    );
}

#[rstest]
#[tokio::test]
async fn test_check_missing_payload_recover(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let repo2 = crate::fixtures::tmprepo("fs").await;

    let manifest = generate_tree(&tmprepo).await.to_graph_manifest();
    let digest = manifest.digest().unwrap();
    crate::Syncer::new(&tmprepo.repo(), &repo2.repo())
        .sync_digest(digest)
        .await
        .expect("Failed to sync repos");

    let file = manifest
        .iter_entries()
        .find(|entry| entry.is_regular_file())
        .expect("at least one regular file");

    tracing::info!(digest=%file.object(), "remove payload");
    tmprepo
        .remove_payload(*file.object())
        .await
        .expect("failed to remove payload");

    let total_blobs = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .count();
    let total_objects = total_blobs + 1; //the manifest

    let results = Checker::new(&tmprepo.repo())
        .with_repair_source(&repo2.repo())
        .check_all_objects()
        .await
        .unwrap();

    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    tracing::info!("{summary:#?}");
    assert_eq!(
        summary.checked_objects, total_objects,
        "expected all items to be visited"
    );
    assert_eq!(
        summary.checked_payloads, total_blobs,
        "expected all payloads to be visited after repair"
    );
    assert!(
        summary.missing_payloads.is_empty(),
        "should repair missing payload"
    );
    assert_eq!(
        summary.repaired_payloads, 1,
        "should repair missing payload"
    );
    assert!(
        summary.missing_objects.is_empty(),
        "should see no missing objects"
    );
}

#[rstest]
#[tokio::test]
async fn test_check_missing_object_recover(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let repo2 = crate::fixtures::tmprepo("fs").await;

    let manifest = generate_tree(&tmprepo).await.to_graph_manifest();
    let digest = manifest.digest().unwrap();
    crate::Syncer::new(&tmprepo.repo(), &repo2.repo())
        .sync_digest(digest)
        .await
        .expect("Failed to sync repos");

    let file = manifest
        .iter_entries()
        .find(|entry| entry.is_regular_file())
        .expect("at least one regular file");

    tracing::info!(digest=%file.object(), "remove object");
    tmprepo
        .remove_payload(*file.object())
        .await
        .expect("failed to remove object");

    let total_blobs = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .count();
    let total_objects = total_blobs + 1; //the manifest

    let results = Checker::new(&tmprepo.repo())
        .with_repair_source(&repo2.repo())
        .check_all_objects()
        .await
        .unwrap();

    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    tracing::info!("{summary:#?}");
    assert_eq!(
        summary.checked_objects, total_objects,
        "expected all items to be visited after repair"
    );
    assert_eq!(
        summary.checked_payloads, total_blobs,
        "all payloads should be seen after repair"
    );
    assert!(
        summary.missing_objects.is_empty(),
        "should repair missing object"
    );
    assert_eq!(summary.repaired_objects, 1, "should repair missing object");
    assert!(
        summary.missing_payloads.is_empty(),
        "should see no missing payloads",
    );
}

#[derive(Default)]
struct DebugReporter {
    checked_object_results: Mutex<Vec<super::CheckObjectResult>>,
}

impl CheckReporter for &DebugReporter {
    fn checked_object(&self, result: &super::CheckObjectResult) {
        self.checked_object_results
            .lock()
            .unwrap()
            .push(result.clone());
    }
}

/// A check on a repo that is missing an annotation blob.
///
/// The check should complete successfully and report a missing object.
#[rstest]
#[tokio::test]
async fn check_missing_annotation_blob(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;

    let blob = tmprepo
        .commit_blob(Box::pin(b"this is some data".as_slice()))
        .await
        .unwrap();

    let layer = crate::graph::Layer::new_with_annotation(
        "test_annotation",
        crate::graph::AnnotationValue::Blob(blob.into()),
    );

    tmprepo.write_object(&layer).await.unwrap();

    // Checking assumptions about starting state of repo.
    {
        let results = Checker::new(&tmprepo.repo())
            .check_all_objects()
            .await
            .unwrap();

        let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
        tracing::info!("{summary:#?}");
        assert_eq!(summary.checked_objects, 2);
        assert_eq!(summary.checked_payloads, 1);
    }

    // Remove the blob backing the annotation.
    tmprepo.remove_object(blob).await.unwrap();

    let debug_reporter = DebugReporter::default();

    let results = Checker::new(&tmprepo.repo())
        .with_reporter(&debug_reporter)
        .check_all_objects()
        .await
        .expect("checker should succeed when an annotation blob is missing");

    let summary: CheckSummary = results.iter().map(|r| r.summary()).sum();
    tracing::info!("{summary:#?}");
    assert_eq!(summary.checked_objects, 2);
    assert_eq!(summary.checked_payloads, 0);
    assert!(
        summary.missing_objects.contains(&blob),
        "should report missing annotation blob"
    );

    let checked_objects = debug_reporter.checked_object_results.lock().unwrap();
    assert_eq!(checked_objects.len(), 2);
    // Confirm there is a Missing result for the annotation blob
    assert!(
        checked_objects
            .iter()
            .any(|r| matches!(r, super::CheckObjectResult::Missing(digest) if *digest == blob))
    );
}
