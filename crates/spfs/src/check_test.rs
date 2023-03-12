// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rand::distributions::{Alphanumeric, DistString};
use rand::Rng;
use rstest::rstest;
use tempfile::tempdir;

use super::{CheckSummary, Checker};
use crate::encoding::prelude::*;
use crate::fixtures::*;
use crate::tracking;

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

    tracing::info!(digest=?file.object, "remove payload");
    tmprepo
        .remove_payload(file.object)
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
        summary.missing_payloads.contains(&file.object),
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

    tracing::info!(digest=?file.object, "remove object");
    tmprepo
        .remove_object(file.object)
        .await
        .expect("failed to remove object");

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
        summary.missing_objects.contains(&file.object),
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

    tracing::info!(digest=?file.object, "remove payload");
    tmprepo
        .remove_payload(file.object)
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

    tracing::info!(digest=?file.object, "remove object");
    tmprepo
        .remove_object(file.object)
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

async fn generate_tree(tmprepo: &TempRepo) -> tracking::Manifest {
    let tmpdir = tempdir().expect("failed to create tempdir");

    let mut rng = rand::thread_rng();
    let max_depth = rng.gen_range(2..6);

    generate_subtree(tmpdir.path(), max_depth);
    crate::commit_dir(tmprepo.repo(), tmpdir.path())
        .await
        .expect("Failed to commit generated tree")
}

fn generate_subtree(root: &std::path::Path, max_depth: i32) {
    let mut rng = rand::thread_rng();
    let dirs = rng.gen_range(2..6);
    let files = rng.gen_range(2..6);

    for _file in 0..files {
        let name_len = rng.gen_range(4..16);
        let name = Alphanumeric.sample_string(&mut rng, name_len);
        let data_len = rng.gen_range(8..64);
        let data = Alphanumeric.sample_string(&mut rng, data_len);
        std::fs::write(root.join(name), data).expect("Failed to generate file");
    }

    if max_depth > 1 {
        for _dir in 0..dirs {
            let name_len = rng.gen_range(4..16);
            let name = Alphanumeric.sample_string(&mut rng, name_len);
            let path = root.join(name);
            std::fs::create_dir_all(&path).expect("Failed to generate subdir");
            generate_subtree(&path, max_depth - 1);
        }
    }
}
