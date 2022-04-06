// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Publisher;
use crate::{api, fixtures::*};

#[rstest]
fn test_publish_no_version_spec() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    rt.tmprepo.publish_spec(spec).unwrap();
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            spec.clone(),
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .unwrap();

    let destination = crate::HANDLE.block_on(spfsrepo());
    let publisher = Publisher::new(rt.tmprepo.clone(), destination.repo.clone());
    publisher.publish(&spec.pkg.with_build(None)).unwrap();
    destination.get_package(&spec.pkg).unwrap();
}
