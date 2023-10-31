// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use rstest::rstest;
use tokio_stream::StreamExt;

use crate::clean::TracingCleanReporter;
use crate::fixtures::*;
use crate::{encoding, storage, tracking, Cleaner, Error};

#[rstest]
#[tokio::test]
async fn test_prunable_tags_age(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;
    let mut old = tracking::Tag::new(
        Some("testing".to_string()),
        "prune",
        encoding::NULL_DIGEST.into(),
    )
    .unwrap();
    old.parent = encoding::NULL_DIGEST.into();
    old.time = Utc.timestamp_opt(10000, 0).unwrap();
    let cutoff = Utc.timestamp_opt(20000, 0).unwrap();
    let mut new = tracking::Tag::new(
        Some("testing".to_string()),
        "prune",
        encoding::EMPTY_DIGEST.into(),
    )
    .unwrap();
    new.parent = encoding::EMPTY_DIGEST.into();
    new.time = Utc.timestamp_opt(30000, 0).unwrap();
    tmprepo.insert_tag(&old).await.unwrap();
    tmprepo.insert_tag(&new).await.unwrap();

    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_dry_run(true)
        .with_prune_tags_older_than(Some(cutoff));
    let result = cleaner.prune_all_tags_and_clean().await.unwrap();
    let tags = result.into_all_tags();
    assert!(tags.contains(&old));
    assert!(!tags.contains(&new));

    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_dry_run(true)
        .with_prune_tags_older_than(Some(cutoff))
        .with_keep_tags_newer_than(Some(Utc.timestamp_opt(0, 0).unwrap()));
    let result = cleaner.prune_all_tags_and_clean().await.unwrap();
    let tags = result.into_all_tags();
    assert!(!tags.contains(&old), "should prefer to keep when ambiguous");
    assert!(!tags.contains(&new));
}

#[rstest]
#[tokio::test]
async fn test_prunable_tags_version(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;
    let tag = tracking::TagSpec::parse("testing/versioned").unwrap();
    let tag5 = tmprepo
        .push_tag(&tag, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();
    let tag4 = tmprepo
        .push_tag(&tag, &encoding::NULL_DIGEST.into())
        .await
        .unwrap();
    let tag3 = tmprepo
        .push_tag(&tag, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();
    let tag2 = tmprepo
        .push_tag(&tag, &encoding::NULL_DIGEST.into())
        .await
        .unwrap();
    let tag1 = tmprepo
        .push_tag(&tag, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();
    let tag0 = tmprepo
        .push_tag(&tag, &encoding::NULL_DIGEST.into())
        .await
        .unwrap();

    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_dry_run(true)
        .with_prune_tags_if_version_more_than(Some(2));
    let result = cleaner.prune_all_tags_and_clean().await.unwrap();
    let tags = result.into_all_tags();
    assert!(!tags.contains(&tag0));
    assert!(!tags.contains(&tag1));
    assert!(!tags.contains(&tag2));
    assert!(tags.contains(&tag3));
    assert!(tags.contains(&tag4));
    assert!(tags.contains(&tag5));

    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_dry_run(true)
        .with_prune_tags_if_version_more_than(Some(2))
        .with_keep_tags_if_version_less_than(Some(4));
    let result = cleaner.prune_all_tags_and_clean().await.unwrap();
    let tags = result.into_all_tags();
    assert!(!tags.contains(&tag0));
    assert!(!tags.contains(&tag1));
    assert!(!tags.contains(&tag2));
    assert!(
        !tags.contains(&tag3),
        "should prefer to keep in ambiguous situation"
    );
    assert!(tags.contains(&tag4));
    assert!(tags.contains(&tag5));
}

#[rstest]
#[tokio::test]
async fn test_prune_tags(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let tag = tracking::TagSpec::parse("test/prune").unwrap();

    async fn reset(tmprepo: &storage::RepositoryHandle) -> HashMap<i32, tracking::Tag> {
        let tag = tracking::TagSpec::parse("test/prune").unwrap();
        let mut tags = HashMap::new();
        match tmprepo.remove_tag_stream(&tag).await {
            Ok(_) | Err(Error::UnknownReference(_)) => (),
            Err(err) => panic!("{err:?}"),
        }

        for year in vec![2020, 2021, 2022, 2023, 2024, 2025].into_iter() {
            let time = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap();
            let digest = random_digest();
            let mut tag = tracking::Tag::new(Some("test".into()), "prune", digest).unwrap();
            tag.time = time;
            tmprepo.insert_tag(&tag).await.unwrap();
            tags.insert(year, tag);
        }
        tags
    }

    let tags = reset(&tmprepo).await;
    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_prune_tags_older_than(Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()));
    cleaner.prune_all_tags_and_clean().await.unwrap();
    let mut tag_stream = tmprepo.read_tag(&tag).await.unwrap();
    while let Some(tag) = tag_stream.next().await {
        assert_eq!(
            &tag.unwrap(),
            tags.get(&2025).unwrap(),
            "should remove all but 2025"
        );
    }

    let tags = reset(&tmprepo).await;
    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_prune_tags_if_version_more_than(Some(2));
    println!("{}", cleaner.format_plan());
    cleaner.prune_all_tags_and_clean().await.unwrap();
    let mut tag_stream = tmprepo.read_tag(&tag).await.unwrap();
    while let Some(tag) = tag_stream.try_next().await.unwrap() {
        assert_ne!(
            &tag,
            tags.get(&2020).unwrap(),
            "should remove 20, 21, and 22"
        );
        assert_ne!(
            &tag,
            tags.get(&2021).unwrap(),
            "should remove 20, 21, and 22"
        );
        assert_ne!(
            &tag,
            tags.get(&2022).unwrap(),
            "should remove 20, 21, and 22"
        );
    }

    let _tags = reset(&tmprepo).await;
    let cleaner = Cleaner::new(&tmprepo)
        .with_reporter(TracingCleanReporter)
        .with_prune_tags_older_than(Some(Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap()));
    cleaner.prune_all_tags_and_clean().await.unwrap();
    if tmprepo.read_tag(&tag).await.is_ok() {
        panic!("should not have any pruned tag left")
    }
}

fn random_digest() -> encoding::Digest {
    use rand::Rng;
    let mut hasher = encoding::Hasher::new_sync();
    let mut rng = rand::thread_rng();
    let mut buf = vec![0; 64];
    rng.fill(buf.as_mut_slice());
    hasher.update(buf.as_slice());
    hasher.digest()
}
