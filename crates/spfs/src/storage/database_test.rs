// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use crate::{encoding, graph};

use crate::fixtures::*;

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_object_existence(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let digest = encoding::EMPTY_DIGEST.into();
    let obj = graph::Blob::new(digest, 0).into();
    tmprepo
        .write_object(&obj)
        .await
        .expect("failed to write object data");

    let actual = tmprepo.has_object(digest).await;
    assert!(actual);

    tmprepo.remove_object(digest).await.unwrap();

    let actual = tmprepo.has_object(digest).await;
    assert!(!actual, "object should not exist after being removed");
}
