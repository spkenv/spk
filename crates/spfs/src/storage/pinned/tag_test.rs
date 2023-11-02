// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use chrono::Utc;
use futures::{StreamExt, TryStreamExt};
use relative_path::RelativePath;
use rstest::rstest;
use spfs_encoding as encoding;

use super::PinnedRepository;
use crate::fixtures::*;
use crate::storage::TagStorage;
use crate::Error;

#[rstest]
#[tokio::test]
async fn test_pinned_limit() {
    // test that a tag stream is limited to the
    // pinned time when read from a pinned storage instance

    let tmprepo = tmprepo("fs").await;

    let spec = "spfs-test".parse().unwrap();
    let start = Utc::now();
    let first = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();
    let after_first = Utc::now();
    let second = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();
    let after_second = Utc::now();
    let third = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();

    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), Utc::now())
            .resolve_tag(&spec)
            .await
            .unwrap(),
        third,
        "should resolve to the latest before the pinned time"
    );
    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_second)
            .resolve_tag(&spec)
            .await
            .unwrap(),
        second,
        "should resolve to the latest before the pinned time"
    );
    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_first)
            .resolve_tag(&spec)
            .await
            .unwrap(),
        first,
        "should resolve to the latest before the pinned time"
    );
    assert!(
        matches!(
            PinnedRepository::new(tmprepo.repo(), start)
                .resolve_tag(&spec)
                .await,
            Err(Error::UnknownReference(_))
        ),
        "should error when the pinned time is before the tag was created"
    );
}

#[rstest]
#[tokio::test]
async fn test_pinned_numbering() {
    // test that tags are properly renumbered relative
    // to the pinned present time

    let tmprepo = tmprepo("fs").await;

    let spec = "spfs-test".parse().unwrap();
    let _start = Utc::now();
    let first = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();
    let _after_first = Utc::now();
    let _second = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();
    let after_second = Utc::now();
    let _third = tmprepo.push_tag(&spec, &random_digest()).await.unwrap();

    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), Utc::now())
            .resolve_tag(&spec.with_version(2))
            .await
            .unwrap(),
        first,
        "should lookup tags relative to the pinned time, not absolute time"
    );
    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_second)
            .resolve_tag(&spec.with_version(1))
            .await
            .unwrap(),
        first,
        "should lookup tags relative to the pinned time, not absolute time"
    );
    assert!(
        matches!(
            PinnedRepository::new(tmprepo.repo(), after_second)
                .resolve_tag(&spec.with_version(2))
                .await,
            Err(Error::UnknownReference(_))
        ),
        "should error when the pinned time is before the tag exists"
    );
}

#[rstest]
#[tokio::test]
async fn test_pinned_visibility() {
    // test that tags are not iterable or visible in ls-tags
    // when they contain entries that are only newer than the
    // pinned time

    let tmprepo = tmprepo("fs").await;

    let spec1 = "spfs-test1".parse().unwrap();
    let spec2 = "spfs-test2".parse().unwrap();
    let spec3 = "spfs-test3".parse().unwrap();
    let _start = Utc::now();
    let first = tmprepo.push_tag(&spec1, &random_digest()).await.unwrap();
    let after_first = Utc::now();
    let _second = tmprepo.push_tag(&spec2, &random_digest()).await.unwrap();
    let after_second = Utc::now();
    let _third = tmprepo.push_tag(&spec3, &random_digest()).await.unwrap();

    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_first)
            .iter_tags()
            .try_collect::<Vec<_>>()
            .await
            .unwrap(),
        vec![(spec1.clone(), first)],
        "should not yield tags that don't exist yet"
    );
    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_first)
            .iter_tag_streams()
            .count()
            .await,
        1,
        "should not yield streams that don't exist yet"
    );
    assert_eq!(
        PinnedRepository::new(tmprepo.repo(), after_second)
            .ls_tags(RelativePath::new("/"))
            .map_ok(Into::<String>::into)
            .try_collect::<HashSet<_>>()
            .await
            .unwrap(),
        HashSet::from([spec1.to_string(), spec2.to_string()]),
        "should not show entries for tags that don't exist yet"
    );
}

fn random_digest() -> encoding::Digest {
    use rand::Rng;
    let mut hasher = encoding::Hasher::<std::io::Sink>::default();
    let mut rng = rand::thread_rng();
    let mut buf = vec![0; 64];
    rng.fill(buf.as_mut_slice());
    hasher.update(buf.as_slice());
    hasher.digest()
}
