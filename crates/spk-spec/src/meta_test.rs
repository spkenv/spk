// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::v0;

#[rstest]
fn test_package_meta_missing() {
    let spec: serde_yaml::Result<v0::Spec> = serde_yaml::from_str(
        r#"{
        pkg: meta/1.0.0
    }"#,
    );
    assert!(spec.is_ok());
    assert_eq!(
        spec.unwrap().meta.license,
        crate::meta::Meta::default_license()
    );
}

#[rstest]
fn test_package_meta_basic() {
    let meta: super::Meta = serde_yaml::from_str(
        r#"
        description: package description
        labels:
            department: fx
    "#,
    )
    .unwrap();
    assert_eq!(meta.license, crate::meta::Meta::default_license());
    assert!(meta.description.is_some());
    assert!(meta.homepage.is_none());
    assert!(meta.labels.contains_key("department"));
}
