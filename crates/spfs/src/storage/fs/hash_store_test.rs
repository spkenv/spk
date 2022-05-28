// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
        $crate::Digest::from_bytes(
            &data_encoding::BASE32
                .decode(format!("{:A<digest_size$}", $digest, digest_size = 56).as_bytes())
                .expect("decode as base32")[..$crate::encoding::DIGEST_SIZE],
        )
        .expect("from_bytes")
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
    for starts_with in ["A", "AB", "ABC", "ABE", "BB", "DD"] {
        let mut matches = Vec::new();
        let mut stream = Box::pin(store.find(DigestSearchCriteria::StartsWith(starts_with.into())));
        while let Some(Ok(v)) = stream.next().await {
            matches.push(v);
        }
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
        // `matches` should be empty after above loop.
        assert!(
            matches.is_empty(),
            "Using StartsWith({}), got unexpected matches: {:?}",
            starts_with,
            matches
        )
    }
}
