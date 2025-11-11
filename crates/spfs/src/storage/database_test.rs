// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use rstest::rstest;

use crate::encoding::PartialDigest;
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

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn resolve_partial_digest_with_blob_object_file_present(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    let tmprepo = tmprepo.await;

    let test_data = b"test data\n";

    let payload = tmprepo
        .commit_payload(Box::pin(test_data.as_slice()))
        .await
        .expect("failed to commit payload data");

    let partial_digest = PartialDigest::from(&payload.as_bytes()[..8]);

    // First test baseline behavior without blob object file present

    // PartialDigestType::Unknown
    {
        let resolved = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Unknown)
            .await
            .expect("failed to resolve partial digest");

        assert_eq!(*resolved.digest(), payload);
        assert!(matches!(resolved, graph::FoundDigest::Payload(_)));
    }

    // PartialDigestType::Payload
    {
        let resolved = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Payload)
            .await
            .expect("failed to resolve partial digest");

        assert_eq!(*resolved.digest(), payload);
        assert!(matches!(resolved, graph::FoundDigest::Payload(_)));
    }

    // PartialDigestType::Object
    {
        let _ = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Object)
            .await
            .expect_err("no object with this digest should exist");
    }

    // Write the blob object file
    {
        let blob = graph::Blob::new(payload, test_data.len() as u64);

        // Safety: we are writing a blob object for the purposes of this test
        unsafe {
            tmprepo
                .write_object_unchecked(&blob)
                .await
                .expect("failed to write blob object data");
        }
    }

    // Then test behavior with blob object file present

    // PartialDigestType::Unknown
    {
        let resolved = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Unknown)
            .await
            .expect("failed to resolve partial digest");

        assert_eq!(*resolved.digest(), payload);
        assert!(matches!(resolved, graph::FoundDigest::Payload(_)));
    }

    // PartialDigestType::Payload
    {
        let resolved = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Payload)
            .await
            .expect("failed to resolve partial digest");

        assert_eq!(*resolved.digest(), payload);
        assert!(matches!(resolved, graph::FoundDigest::Payload(_)));
    }

    // PartialDigestType::Object
    {
        let resolved = tmprepo
            .resolve_full_digest(&partial_digest, graph::PartialDigestType::Object)
            .await
            .expect("failed to resolve partial digest");

        assert_eq!(*resolved.digest(), payload);
        assert!(matches!(resolved, graph::FoundDigest::Object(_)));
    }
}
