// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use tokio::io::AsyncReadExt;

use rstest::rstest;

use crate::fixtures::*;

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_payload_io(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;
    let bytes = "simple string data".as_bytes();
    let reader = Box::pin(bytes.clone());

    let (digest, size) = tmprepo
        .write_data(reader)
        .await
        .expect("failed to write payload data");
    assert_eq!(size, bytes.len() as u64);

    let mut actual = String::new();
    tmprepo
        .open_payload(digest)
        .await
        .unwrap()
        .read_to_string(&mut actual)
        .await
        .unwrap();
    assert_eq!(&actual, "simple string data");
}
