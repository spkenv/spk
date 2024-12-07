// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rstest::rstest;
use storage::prelude::*;
use tokio::time::sleep;

use super::{Cleaner, TracingCleanReporter};
use crate::encoding::prelude::*;
use crate::fixtures::*;
use crate::storage::fs::RenderStore;
use crate::{Error, storage, tracking};

#[rstest]
#[tokio::test]
async fn test_attached_objects(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;

    let manifest = generate_tree(&tmprepo).await.to_graph_manifest();

    let cleaner = Cleaner::new(&tmprepo).with_reporter(TracingCleanReporter);
    cleaner
        .visit_attached_objects(manifest.digest().unwrap())
        .await
        .unwrap();

    let total_blobs = manifest
        .iter_entries()
        .filter(|e| e.is_regular_file())
        .count();
    let total_objects = total_blobs + 1; //the manifest

    assert_eq!(cleaner.attached.len(), total_objects);
}

#[rstest]
#[tokio::test]
async fn test_get_attached_unattached_objects_blob(
    #[future] tmprepo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    let data_dir = tmpdir.path().join("data");
    ensure(data_dir.join("file.txt"), "hello, world");

    let manifest = crate::Committer::new(&tmprepo)
        .commit_dir(data_dir.as_path())
        .await
        .unwrap();
    let layer = tmprepo
        .create_layer(&manifest.to_graph_manifest())
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("my_tag").unwrap();
    tmprepo
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();
    let blob_digest = manifest
        .root()
        .entries
        .get("file.txt")
        .expect("file should exist in committed manifest")
        .object;

    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_dry_run(true);
    let result = cleaner.prune_all_tags_and_clean().await.unwrap();
    println!("{result:#?}");

    assert!(
        cleaner.attached.contains(&blob_digest),
        "blob in manifest in tag should be attached"
    );
}

#[rstest]
#[tokio::test]
async fn test_clean_untagged_objects(#[future] tmprepo: TempRepo, tmpdir: tempfile::TempDir) {
    init_logging();
    let tmprepo = tmprepo.await;

    // Group 1: untagged objects
    let data_dir_1 = tmpdir.path().join("data");
    ensure(data_dir_1.join("dir/dir/test.file"), "1 hello");
    ensure(data_dir_1.join("dir/dir/test.file2"), "1 hello, world");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 hello, world");

    let manifest1 = crate::Committer::new(&tmprepo)
        .commit_dir(data_dir_1.as_path())
        .await
        .unwrap();

    // Group 2: tagged objects
    let data_dir_2 = tmpdir.path().join("data2");
    ensure(data_dir_2.join("dir/dir/test.file"), "2 hello");
    ensure(data_dir_2.join("dir/dir/test.file2"), "2 hello, world");

    let manifest2 = crate::Committer::new(&tmprepo)
        .commit_dir(data_dir_2.as_path())
        .await
        .unwrap();
    let layer = tmprepo
        .create_layer(&manifest2.to_graph_manifest())
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("tagged_manifest").unwrap();
    tmprepo
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();

    // Note current time now.
    let time_before_group_three = Utc::now();

    // Ensure these new files are created a measurable amount of time after
    // the noted time.
    sleep(Duration::from_millis(250)).await;

    // Group 3: untagged objects created after grabbing time.
    let data_dir_3 = tmpdir.path().join("data");
    ensure(data_dir_3.join("dir/dir/test.file"), "3 hello");
    ensure(data_dir_3.join("dir/dir/test.file2"), "3 hello, world");
    ensure(data_dir_3.join("dir/dir/test.file4"), "3 hello, world");

    let manifest3 = crate::Committer::new(&tmprepo)
        .commit_dir(data_dir_3.as_path())
        .await
        .unwrap();

    // Clean objects older than group 3.
    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_required_age_cutoff(time_before_group_three);
    let result = cleaner
        .prune_all_tags_and_clean()
        .await
        .expect("failed to clean objects");
    println!("{result:#?}");

    for node in manifest1.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        let res = tmprepo.open_payload(node.entry.object).await;
        if let Err(Error::UnknownObject(_)) = res {
            continue;
        }
        if let Err(err) = res {
            println!("{err:?}");
        }
        panic!(
            "expected object to be cleaned but it was not: {:?}",
            node.entry.object
        );
    }

    for node in manifest2.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }

    // Group 3 should not have been cleaned...

    for node in manifest3.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }
}

#[rstest]
#[tokio::test]
async fn test_clean_on_repo_with_tag_namespace(
    #[future] tmprepo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    init_logging();

    // The point of this test is to run a cleaner when a tag in a tag namespace
    // exists. It is a dupe of `test_clean_untagged_objects` but the content is
    // created in a tag namespace while the cleaner is run on the non-namespaced
    // repo.

    let non_namespaced_tmprepo = tmprepo.await;
    let namespaced_tmprepo = non_namespaced_tmprepo.with_tag_namespace("test").await;

    // Group 1: untagged objects
    let data_dir_1 = tmpdir.path().join("data");
    ensure(data_dir_1.join("dir/dir/test.file"), "1 hello");
    ensure(data_dir_1.join("dir/dir/test.file2"), "1 hello, world");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 hello, world");

    let manifest1 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_1.as_path())
        .await
        .unwrap();

    // Group 2: tagged objects
    let data_dir_2 = tmpdir.path().join("data2");
    ensure(data_dir_2.join("dir/dir/test.file"), "2 hello");
    ensure(data_dir_2.join("dir/dir/test.file2"), "2 hello, world");

    let manifest2 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_2.as_path())
        .await
        .unwrap();
    let layer = namespaced_tmprepo
        .create_layer(&manifest2.to_graph_manifest())
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("tagged_manifest").unwrap();
    namespaced_tmprepo
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();

    // Note current time now.
    let time_before_group_three = Utc::now();

    // Ensure these new files are created a measurable amount of time after
    // the noted time.
    sleep(Duration::from_millis(250)).await;

    // Group 3: untagged objects created after grabbing time.
    let data_dir_3 = tmpdir.path().join("data");
    ensure(data_dir_3.join("dir/dir/test.file"), "3 hello");
    ensure(data_dir_3.join("dir/dir/test.file2"), "3 hello, world");
    ensure(data_dir_3.join("dir/dir/test.file4"), "3 hello, world");

    let manifest3 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_3.as_path())
        .await
        .unwrap();

    // Clean objects older than group 3.
    let cleaner = Cleaner::new(&non_namespaced_tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_required_age_cutoff(time_before_group_three);
    let result = cleaner
        .prune_all_tags_and_clean()
        .await
        .expect("failed to clean objects");
    println!("{result:#?}");

    for node in manifest1.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        let res = namespaced_tmprepo.open_payload(node.entry.object).await;
        if let Err(Error::UnknownObject(_)) = res {
            continue;
        }
        if let Err(err) = res {
            println!("{err:?}");
        }
        panic!(
            "expected object to be cleaned but it was not: {:?}",
            node.entry.object
        );
    }

    for node in manifest2.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        namespaced_tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }

    // Group 3 should not have been cleaned...

    for node in manifest3.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        namespaced_tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }
}

#[rstest]
#[tokio::test]
async fn test_clean_on_repo_with_tag_namespace_set(
    #[future] tmprepo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    init_logging();

    // The point of this test is to run a cleaner when a tag in a tag namespace
    // exists and a tag namespace is set on the repo passed into the Cleaner. It
    // is a dupe of `test_clean_on_repo_with_tag_namespace` but the repo passed
    // into the Cleaner has a tag namespace set.

    let non_namespaced_tmprepo = tmprepo.await;
    let namespaced_tmprepo = non_namespaced_tmprepo.with_tag_namespace("test").await;

    // Group 1: untagged objects
    let data_dir_1 = tmpdir.path().join("data");
    ensure(data_dir_1.join("dir/dir/test.file"), "1 hello");
    ensure(data_dir_1.join("dir/dir/test.file2"), "1 hello, world");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 hello, world");

    let manifest1 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_1.as_path())
        .await
        .unwrap();

    // Group 2: tagged objects
    let data_dir_2 = tmpdir.path().join("data2");
    ensure(data_dir_2.join("dir/dir/test.file"), "2 hello");
    ensure(data_dir_2.join("dir/dir/test.file2"), "2 hello, world");

    let manifest2 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_2.as_path())
        .await
        .unwrap();
    let layer = namespaced_tmprepo
        .create_layer(&manifest2.to_graph_manifest())
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("tagged_manifest").unwrap();
    namespaced_tmprepo
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();

    // Note current time now.
    let time_before_group_three = Utc::now();

    // Ensure these new files are created a measurable amount of time after
    // the noted time.
    sleep(Duration::from_millis(250)).await;

    // Group 3: untagged objects created after grabbing time.
    let data_dir_3 = tmpdir.path().join("data");
    ensure(data_dir_3.join("dir/dir/test.file"), "3 hello");
    ensure(data_dir_3.join("dir/dir/test.file2"), "3 hello, world");
    ensure(data_dir_3.join("dir/dir/test.file4"), "3 hello, world");

    let manifest3 = crate::Committer::new(&namespaced_tmprepo)
        .commit_dir(data_dir_3.as_path())
        .await
        .unwrap();

    // Clean objects older than group 3.
    let cleaner = Cleaner::new(&namespaced_tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_required_age_cutoff(time_before_group_three);
    let result = cleaner
        .prune_all_tags_and_clean()
        .await
        .expect("failed to clean objects");
    println!("{result:#?}");

    for node in manifest1.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        let res = namespaced_tmprepo.open_payload(node.entry.object).await;
        if let Err(Error::UnknownObject(_)) = res {
            continue;
        }
        if let Err(err) = res {
            println!("{err:?}");
        }
        panic!(
            "expected object to be cleaned but it was not: {:?}",
            node.entry.object
        );
    }

    for node in manifest2.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        namespaced_tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }

    // Group 3 should not have been cleaned...

    for node in manifest3.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        namespaced_tmprepo
            .open_payload(node.entry.object)
            .await
            .expect("expected payload not to be cleaned");
    }
}

#[rstest]
#[tokio::test]
async fn test_clean_untagged_objects_layers_platforms(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let manifest = tracking::Manifest::<()>::default();
    let layer = tmprepo
        .create_layer(&manifest.to_graph_manifest())
        .await
        .unwrap();
    let platform = tmprepo
        .create_platform(layer.digest().unwrap().into())
        .await
        .unwrap();

    let cleaner = Cleaner::new(&tmprepo).with_reporter(TracingCleanReporter);
    let result = cleaner
        .prune_all_tags_and_clean()
        .await
        .expect("failed to clean objects");
    println!("{result:#?}");

    if let Err(Error::UnknownObject(_)) = tmprepo.read_layer(layer.digest().unwrap()).await {
        // ok
    } else {
        panic!("expected layer to be cleaned")
    }

    if let Err(Error::UnknownObject(_)) = tmprepo.read_platform(platform.digest().unwrap()).await {
        // ok
    } else {
        panic!("expected platform to be cleaned")
    }
}

#[rstest]
#[tokio::test]
async fn test_clean_manifest_renders(tmpdir: tempfile::TempDir) {
    init_logging();
    let tmprepo = Arc::new(
        storage::fs::MaybeOpenFsRepository::<RenderStore>::create(tmpdir.path())
            .await
            .unwrap()
            .into(),
    );

    let data_dir = tmpdir.path().join("data");
    ensure(data_dir.join("dir/dir/file.txt"), "hello");
    ensure(data_dir.join("dir/name.txt"), "john doe");

    let manifest = crate::Committer::new(&tmprepo)
        .commit_dir(data_dir.as_path())
        .await
        .unwrap();
    let layer = tmprepo
        .create_layer(&manifest.to_graph_manifest())
        .await
        .unwrap();
    let _platform = tmprepo
        .create_platform(layer.digest().unwrap().into())
        .await
        .unwrap();

    let fs_repo = match &*tmprepo {
        RepositoryHandle::FSWithRenders(fs) => fs,
        _ => panic!("Unexpected tmprepo type!"),
    };
    let fs_repo = fs_repo.opened().await.unwrap();

    storage::fs::Renderer::new(&fs_repo)
        .render_manifest(&manifest.to_graph_manifest(), None)
        .await
        .unwrap();

    let files = list_files(fs_repo.objects.root());
    assert!(!files.is_empty(), "should have stored data");

    let cleaner = Cleaner::new(&tmprepo).with_reporter(TracingCleanReporter);
    let result = cleaner
        .prune_all_tags_and_clean()
        .await
        .expect("failed to clean repo");
    println!("{result:#?}");

    let files = list_files(fs_repo.rs_impl.renders.root());
    assert_eq!(
        files,
        Vec::<String>::new(),
        "should remove all created data files"
    );
}

fn list_files<P: AsRef<std::path::Path>>(dirname: P) -> Vec<String> {
    let mut all_files = Vec::new();

    for entry in walkdir::WalkDir::new(dirname) {
        let entry = entry.expect("error while listing dir recursively");
        if entry.metadata().unwrap().is_dir() {
            continue;
        }
        all_files.push(entry.path().to_owned().to_string_lossy().to_string())
    }
    all_files
}
