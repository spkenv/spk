// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use tokio::io::AsyncReadExt;

use futures::TryStreamExt;
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

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_payload_existence(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;
    let bytes = "simple string data".as_bytes();
    let reader = Box::pin(bytes.clone());

    let (digest, size) = tmprepo
        .write_data(reader)
        .await
        .expect("failed to write payload data");
    assert_eq!(size, bytes.len() as u64);

    let actual = tmprepo.has_payload(digest).await;
    assert!(actual);

    tmprepo.remove_payload(digest).await.unwrap();

    let actual = tmprepo.has_payload(digest).await;
    assert!(!actual, "payload should not exist after being removed");
}

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_payloads_iter(#[future] tmprepo: TempRepo) {
    let tmprepo = tmprepo.await;
    let payloads = [
        "simple string data 1".as_bytes(),
        "simple string data 2".as_bytes(),
        "simple string data 3".as_bytes(),
    ];

    let reader_0 = Box::pin(payloads[0].clone());
    let reader_1 = Box::pin(payloads[1].clone());
    let reader_2 = Box::pin(payloads[2].clone());

    let mut expected = vec![
        tmprepo
            .write_data(reader_0)
            .await
            .expect("failed to write payload data")
            .0,
        tmprepo
            .write_data(reader_1)
            .await
            .expect("failed to write payload data")
            .0,
        tmprepo
            .write_data(reader_2)
            .await
            .expect("failed to write payload data")
            .0,
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
