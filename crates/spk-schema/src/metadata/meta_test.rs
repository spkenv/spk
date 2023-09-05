// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema_ident::{AnyIdent, VersionIdent};

use crate::v0;

#[rstest]
fn test_package_meta_missing() {
    let spec: serde_yaml::Result<v0::Spec<VersionIdent>> = serde_yaml::from_str(
        r#"{
        pkg: meta/1.0.0
    }"#,
    );
    assert!(spec.is_ok());
    assert_eq!(
        spec.unwrap().meta.license,
        crate::metadata::Meta::default_license()
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
    assert_eq!(meta.license, crate::metadata::Meta::default_license());
    assert!(meta.description.is_some());
    assert!(meta.homepage.is_none());
    assert!(meta.labels.contains_key("department"));
}

#[rstest]
fn test_custom_metadata() {
    let mut spec: v0::Spec<AnyIdent> = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    spec.meta
        .update_metadata(&String::from("src/metadata/capture_metadata.sh"), &None)
        .unwrap();

    let keys = ["CWD", "HOSTNAME", "REPO", "SHA1"];
    assert_eq!(spec.meta.labels.len(), 4);
    for key in keys.iter() {
        assert!(spec.meta.labels.contains_key(*key));
    }
}
