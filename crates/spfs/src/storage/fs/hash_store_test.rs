// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use tokio_stream::StreamExt;

fixtures!();

#[rstest]
#[tokio::test]
async fn test_hash_store_iter_states(tmpdir: tempdir::TempDir) {
    init_logging();
    let store = super::FSHashStore::open(tmpdir.path()).unwrap();
    let mut stream = store.iter();
    while stream.next().await.is_some() {
        panic!("empty hash store should not yield any digests");
    }
}
