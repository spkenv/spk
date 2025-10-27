// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use rstest::rstest;

use crate::fixtures::*;
use crate::graph;
use crate::prelude::*;

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_object_existence(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;
    let obj = graph::Layer::new_with_annotation(
        "test",
        graph::AnnotationValue::String(Cow::Owned("data".to_owned())),
    );
    let digest = obj.digest().unwrap();
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
