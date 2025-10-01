// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use futures::TryStreamExt;
use rstest::rstest;
use tokio::io::AsyncReadExt;

use crate::fixtures::*;
use crate::prelude::*;

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_payload_io(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let bytes = "simple string data".as_bytes();
    let reader = Box::pin(bytes);

    // Safety: we are intentionally calling this function to test it
    let (digest, size) = unsafe {
        tmprepo
            .write_data(reader)
            .await
            .expect("failed to write payload data")
    };
    assert_eq!(size, bytes.len() as u64);

    let mut actual = String::new();
    tmprepo
        .open_payload(digest)
        .await
        .unwrap()
        .0
        .read_to_string(&mut actual)
        .await
        .unwrap();
    assert_eq!(&actual, "simple string data");
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_payload_existence(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let bytes = "simple string data".as_bytes();
    let reader = Box::pin(bytes);

    // Safety: we are intentionally calling this unsafe function to test it
    let (digest, size) = unsafe {
        tmprepo
            .write_data(reader)
            .await
            .expect("failed to write payload data")
    };
    assert_eq!(size, bytes.len() as u64);

    let actual = tmprepo.has_payload(digest).await;
    assert!(actual);

    tmprepo.remove_payload(digest).await.unwrap();

    let actual = tmprepo.has_payload(digest).await;
    assert!(!actual, "payload should not exist after being removed");
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_payloads_iter(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let payloads = [
        "simple string data 1".as_bytes(),
        "simple string data 2".as_bytes(),
        "simple string data 3".as_bytes(),
    ];

    let reader_0 = Box::pin(payloads[0]);
    let reader_1 = Box::pin(payloads[1]);
    let reader_2 = Box::pin(payloads[2]);

    let mut expected = vec![
        tmprepo
            .commit_payload(reader_0)
            .await
            .expect("failed to write payload data"),
        tmprepo
            .commit_payload(reader_1)
            .await
            .expect("failed to write payload data"),
        tmprepo
            .commit_payload(reader_2)
            .await
            .expect("failed to write payload data"),
    ];
    expected.sort();

    let mut actual = tmprepo
        .iter_payload_digests()
        .try_collect::<Vec<_>>()
        .await
        .expect("failed to iter digests");
    actual.sort();
    assert_eq!(actual, expected, "iter should return all stored digests");
}
