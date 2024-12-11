// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_config::{Metadata, MetadataCommand};
use spk_schema_ident::{AnyIdent, VersionIdent};

use crate::{v0, LintedItem};

#[rstest]
fn test_package_meta_missing() {
    let spec: serde_yaml::Result<v0::Spec<VersionIdent>> = serde_yaml::from_str(
        r#"{
        pkg: meta/1.0.0
    }"#,
    );
    assert!(spec.is_ok());
    assert_eq!(spec.unwrap().meta.license, None);
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
    assert_eq!(meta.license, None);
    assert!(meta.description.is_some());
    assert!(meta.homepage.is_none());
    assert!(meta.labels.contains_key("department"));
}

#[rstest]
fn test_custom_metadata() {
    let mut spec: v0::Spec<AnyIdent> = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    let command = MetadataCommand {
        command: [String::from("src/metadata/test_capture_metadata.sh")].to_vec(),
    };

    let metadata = Metadata {
        global: [command].to_vec(),
    };

    spec.meta.update_metadata(&metadata).unwrap();

    let keys = ["CWD", "HOSTNAME", "REPO", "SHA1"];
    assert_eq!(spec.meta.labels.len(), 4);
    for key in keys.iter() {
        assert!(spec.meta.labels.contains_key(*key));
    }
}

#[rstest]
fn test_meta_description_lints() {
    let meta: LintedItem<super::Meta> = serde_yaml::from_str(
        r#"
            descriptions: package description
        "#,
    )
    .unwrap();

    assert_eq!(meta.lints.len(), 1);
    for lint in meta.lints.iter() {
        assert_eq!(lint.get_key(), "meta.descriptions")
    }
}

#[rstest]
fn test_meta_homepage_lints() {
    let meta: LintedItem<super::Meta> = serde_yaml::from_str(
        r#"
            homepages: www.somerandomhomepage.com
        "#,
    )
    .unwrap();

    assert_eq!(meta.lints.len(), 1);
    for lint in meta.lints.iter() {
        assert_eq!(lint.get_key(), "meta.homepages")
    }
}

#[rstest]
fn test_meta_license_lints() {
    let meta: LintedItem<super::Meta> = serde_yaml::from_str(
        r#"
            licenses: "Hello World!"
        "#,
    )
    .unwrap();

    assert_eq!(meta.lints.len(), 1);
    for lint in meta.lints.iter() {
        assert_eq!(lint.get_key(), "meta.licenses")
    }
}

#[rstest]
fn test_meta_labels_lints() {
    let meta: LintedItem<super::Meta> = serde_yaml::from_str(
        r#"
            label: {}
        "#,
    )
    .unwrap();

    assert_eq!(meta.lints.len(), 1);
    for lint in meta.lints.iter() {
        assert_eq!(lint.get_key(), "meta.label")
    }
}
