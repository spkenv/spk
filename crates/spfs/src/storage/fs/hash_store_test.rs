// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use futures::TryStreamExt;
use rstest::rstest;
use tokio_stream::StreamExt;

use crate::{fixtures::*, graph::DigestSearchCriteria, storage::fs::hash_store::PersistableObject};

#[rstest]
#[tokio::test]
async fn test_hash_store_iter_states(tmpdir: tempdir::TempDir) {
    init_logging();
    let store = super::FSHashStore::open(tmpdir.path()).unwrap();
    let mut stream = Box::pin(store.iter());
    while stream.next().await.is_some() {
        panic!("empty hash store should not yield any digests");
    }
}

/// Produce a `Digest` with the desired string
macro_rules! digest {
    ($digest:expr) => {
        $crate::Digest::parse(&format!("{:A<digest_size$}====", $digest, digest_size = 52))
            .expect("valid digest")
    };
}

#[rstest]
#[tokio::test]
async fn test_hash_store_find_digest(tmpdir: tempdir::TempDir) {
    init_logging();
    let store = super::FSHashStore::open(tmpdir.path()).unwrap();
    let content = ["AAA", "ABC", "ABD", "BBB", "BCD", "CCC", "EEE"];
    for s in content {
        store
            .persist_object_with_digest(PersistableObject::EmptyFile, digest!(s))
            .await
            .expect("persist digest file");
    }
    /*
    // Uncomment to examine store contents.
    let output = std::process::Command::new("/usr/bin/find")
        .arg(tmpdir.path())
        .output()
        .expect("ran");
    std::io::Write::write_all(&mut std::io::stdout(), &output.stdout).expect("write output");
    */
    for starts_with in ["AA", "AB", "ABCA", "ABEA", "BB", "DD"] {
        let partial =
            crate::encoding::PartialDigest::parse(starts_with).expect("valid partial digest");
        let mut matches: Vec<_> = store
            .find(DigestSearchCriteria::StartsWith(partial))
            .try_collect()
            .await
            .expect("should not fail to search");
        let original_matches = matches.clone();
        for control in content {
            if !control.starts_with(starts_with) {
                continue;
            }
            // Remove the element(s) in `matches` that should have been
            // found by this control.
            let len_before = matches.len();
            matches.retain(|el| !el.to_string().starts_with(control));
            // Something should have been removed.
            assert!(
                len_before > matches.len(),
                "Using StartsWith({}), {} was not found in matches: {:?}",
                starts_with,
                control,
                original_matches
            );
        }
        // because of base32 putting partial bytes into the final
        // character, we can't be certain that the last character
        // will be matched exactly
        let unambiguous_query = &starts_with[..starts_with.len() - 1];
        matches.retain(|el| !el.to_string().starts_with(unambiguous_query));
        assert!(
            matches.is_empty(),
            "Using StartsWith({}), got unexpected matches: {:?}",
            starts_with,
            matches
        )
    }
}
