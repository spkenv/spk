// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Publisher;
use crate::{api, fixtures::*};

#[rstest]
#[tokio::test]
async fn test_publish_no_version_spec() {
    let _guard = crate::MUTEX.lock().await;
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    rt.tmprepo.publish_spec(spec).await.unwrap();
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            spec.clone(),
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
