// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Publisher;
use crate::{api, fixtures::*};

#[rstest]
#[tokio::test]
async fn test_publish_no_version_spec() {
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let destination = spfsrepo().await;
    let publisher = Publisher::new(rt.tmprepo.clone(), destination.repo.clone());
    publisher.publish(&spec.pkg.with_build(None)).await.unwrap();
    destination.get_package(&spec.pkg).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_publish_build_also_publishes_spec() {
    // This test only publishes a single build and verifies that the spec
    // is also published.
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let destination = spfsrepo().await;
    let publisher = Publisher::new(rt.tmprepo.clone(), destination.repo.clone());
    // Include build when publishing this spec.
    publisher.publish(&spec.pkg).await.unwrap();
    let r = destination.read_spec(&spec.pkg.with_build(None)).await;
    assert!(
        r.is_ok(),
        "Expected to be able to read spec, but got error: {}",
        r.err().unwrap()
    )
}

#[rstest]
#[tokio::test]
async fn test_publish_multiple_builds_individually() {
    // This test publishes multiple builds and verifies that subsequent builds
    // don't fail to publish because the spec is already there.
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();

    let destination = spfsrepo().await;
    let publisher = Publisher::new(rt.tmprepo.clone(), destination.repo.clone());

    {
        let spec = crate::spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
        rt.tmprepo
            .publish_package(
                &spec,
                vec![(api::Component::Run, empty_layer_digest())]
                    .into_iter()
                    .collect(),
            )
            .await
            .unwrap();

        // Include build when publishing this spec.
        publisher.publish(&spec.pkg).await.unwrap();
    }

    {
        // Publish a second, different build here.
        let spec = crate::spec!({"pkg": "my-pkg/1.0.0/CU7ZWOIF"});
        rt.tmprepo
            .publish_package(
                &spec,
                vec![(api::Component::Run, empty_layer_digest())]
                    .into_iter()
                    .collect(),
            )
            .await
            .unwrap();

        // Include build when publishing this spec.
        let r = publisher.publish(&spec.pkg).await;
        assert!(
            r.is_ok(),
            "Expected second publish to succeed, but got error: {}",
            r.err().unwrap()
        )
    }
}
