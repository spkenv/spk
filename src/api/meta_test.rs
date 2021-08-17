// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

#[rstest]
fn test_package_meta_absent() {
    let spec: serde_yaml::Result<crate::api::Spec> = serde_yaml::from_str(
        r#"{
        pkg: meta/1.0.0
    }"#,
    );
    assert!(spec.is_ok());
    assert!(spec.unwrap().meta.is_none())
}

#[rstest]
fn test_package_meta_basic() {
    let spec: serde_yaml::Result<crate::api::Spec> = serde_yaml::from_str(
        r#"
        pkg: meta/1.0.0
        meta:
            description: package description
            license: MIT
            labels:
                department: fx
    "#,
    );
    assert!(spec.is_ok());
    let spec = spec.unwrap();
    assert!(spec.meta.is_some());
    let meta = spec.meta.unwrap();
    assert!(meta.description.is_some());
    assert!(meta.homepage.is_none());
    assert!(meta.labels.unwrap().contains_key("department"));
}
