use chrono::{DateTime, NaiveDate, Utc};
use rstest::{fixture, rstest};

use super::{get_prunable_tags, prune_tags, PruneParameters};
use crate::{encoding, graph, storage, tracking, Error};
use std::collections::HashMap;

#[rstest]
#[tokio::test]
async fn test_prunable_tags_age(tmprepo: storage::fs::FSRepository) {
    let mut old =
        tracking::Tag::new(Some("testing".to_string()), "prune", encoding::NULL_DIGEST).unwrap();
    old.parent = encoding::NULL_DIGEST;
    old.time = chrono::Utc::timestamp(10000, 0);
    let cutoff = chrono::Utc::timestamp(20000, 0);
    let mut new =
        tracking::Tag::new(Some("testing".to_string()), "prune", encoding::EMPTY_DIGEST).unwrap();
    new.parent = encoding::EMPTY_DIGEST;
    new.time = chrono::Utc::timestamp(30000, 0);
    tmprepo.tags.push_raw_tag(old).unwrap();
    tmprepo.tags.push_raw_tag(new).unwrap();

    let tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_older_than: cutoff.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(tags.contains(old));
    assert!(!tags.contains(new));

    let tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_older_than: cutoff.clone(),
            keep_if_newer_than: chrono::Utc::timestamp(0, 0),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!tags.contains(old), "should prefer to keep when ambiguous");
    assert!(!tags.contains(new));
}

#[rstest]
#[tokio::test]
async fn test_prunable_tags_version(tmprepo: storage::fs::FSRepository) {
    let tag = "testing/versioned";
    let tag5 = tmprepo
        .tags
        .push_tag(&tag, &encoding::EMPTY_DIGEST)
        .unwrap();
    let tag4 = tmprepo.tags.push_tag(&tag, &encoding::NULL_DIGEST).unwrap();
    let tag3 = tmprepo
        .tags
        .push_tag(&tag, &encoding::EMPTY_DIGEST)
        .unwrap();
    let tag2 = tmprepo.tags.push_tag(&tag, &encoding::NULL_DIGEST).unwrap();
    let tag1 = tmprepo
        .tags
        .push_tag(&tag, &encoding::EMPTY_DIGEST)
        .unwrap();
    let tag0 = tmprepo.tags.push_tag(&tag, &encoding::NULL_DIGEST).unwrap();

    let tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_version_more_than: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!tags.contains(tag0));
    assert!(!tags.contains(tag1));
    assert!(!tags.contains(tag2));
    assert!(tags.contains(tag3));
    assert!(tags.contains(tag4));
    assert!(tags.contains(tag5));

    let tags = get_prunable_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_version_more_than: 2,
            keep_if_version_less_than: 4,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!tags.contains(tag0));
    assert!(!tags.contains(tag1));
    assert!(!tags.contains(tag2));
    assert!(
        !tags.contains(tag3),
        "should prefer to keep in ambiguous situation"
    );
    assert!(tags.contains(tag4));
    assert!(tags.contains(tag5));
}

#[rstest]
#[tokio::test]
async fn test_prune_tags(tmprepo: storage::fs::FSRepository) {
    let mut tags = HashMap::new();

    let reset = || {
        match tmprepo.tags.remove_tag_stream("test/prune") {
            Ok(_) | Err(Error::UnknownReference(_)) => (),
            Err(err) => panic!("{:?}", err),
        }

        for year in &[2020, 2021, 2022, 2023, 2024, 2025] {
            let time = NaiveDate::from_ymd(year, 1, 1);
            let digest = random_digest();
            let mut tag = tracking::Tag::new("test", "prune", digest);
            tag.time = time;
            tags.insert(year, tag);
            tmprepo.tags.push_raw_tag(tag).unwrap()
        }
    };

    reset();
    prune_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_older_than: NaiveDate::from_ymd(2024, 1, 1),
            ..Default::default()
        },
    )
    .unwrap();
    for tag in tmprepo.tags.read_tag("test/prune") {
        assert!(Some(tag) != tags.get(2025));
    }

    reset();
    prune_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_version_more_than: 2,
            ..Default::default()
        },
    )
    .unwrap();
    for tag in tmprepo.tags.read_tag("test/prune") {
        assert!(Some(tag) != tags.get(2025));
        assert!(Some(tag) != tags.get(2024));
        assert!(Some(tag) != tags.get(2023));
    }

    reset();
    prune_tags(
        tmprepo.tags,
        PruneParameters {
            prune_if_version_more_than: -1,
            ..Default::default()
        },
    )
    .unwrap();
    if let Ok(_) = tmprepo.tags.read_tag("test/prune") {
        panic!("should not have any pruned tag left")
    }
}

fn random_digest() -> encoding::Digest {
    let mut hasher = encoding::Hasher::new();
    let mut rng = rand::thread_rng();
    let mut buf = Vec::new(64);
    rng.fill(buf.as_mut_slice());
    hasher.update(&buf.as_slice());
    hasher.digest()
}

#[fixture]
fn tmprepo(tmpdir: tempdir::TempDir) -> storage::fs::FSRepository {
    storage::fs::FSRepository::create(tmpdir.path().join("repo")).unwrap()
}

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-test-")
}
