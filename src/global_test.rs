// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{load_spec, save_spec};
use crate::{api::Package, fixtures::*, storage::CachePolicy, with_cache_policy};

#[rstest]
#[tokio::test]
async fn test_load_spec_local() {
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();

    let actual = load_spec("my-pkg").await.unwrap();
    assert_eq!(*actual, spec);
}

#[rstest]
#[tokio::test]
async fn test_save_spec() {
    let rt = spfs_runtime().await;
    let spec = crate::spec!({"pkg": "my-pkg"});

    let res = rt.tmprepo.read_spec(spec.ident()).await;
    assert!(matches!(res, Err(crate::Error::PackageNotFoundError(_))));

    save_spec(&spec).await.unwrap();

    with_cache_policy!(rt.tmprepo, CachePolicy::BypassCache, {
        rt.tmprepo.read_spec(spec.ident())
    })
    .await
    .expect("should exist in repo after saving");
}
