// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{load_spec, save_spec};
use crate::{fixtures::*, storage::CachePolicy, with_cache_policy};

#[rstest]
fn test_load_spec_local() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!({"pkg": "my-pkg"});
    rt.tmprepo.publish_spec(&spec).unwrap();

    let actual = load_spec("my-pkg").unwrap();
    assert_eq!(*actual, spec);
}

#[rstest]
fn test_save_spec() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!({"pkg": "my-pkg"});

    let res = rt.tmprepo.read_spec(&spec.pkg);
    assert!(matches!(res, Err(crate::Error::PackageNotFoundError(_))));

    save_spec(&spec).unwrap();

    with_cache_policy!(rt.tmprepo, CachePolicy::BypassCache, {
        rt.tmprepo.read_spec(&spec.pkg)
    })
    .expect("should exist in repo after saving");
}
