// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
use std::str::FromStr;

use rstest::rstest;
use spk_schema_foundation::ident::PinnableRequest;
use spk_schema_foundation::version::Version;

use crate::{Components, InstallSpec};

#[rstest]
fn test_embedded_components_defaults() {
    // By default, embedded components will embed matching components from the
    // defined embedded packages.
    let install = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
embedded:
  - pkg: "embedded/1.0.0/embedded"
        "#,
    )
    .unwrap();

    assert_eq!(
        install.components.len(),
        2,
        "expecting two default components: build and run"
    );

    assert_eq!(install.embedded.len(), 1, "expecting one embedded package");

    assert_eq!(
        install.embedded[0].components().len(),
        2,
        "expecting two default components: build and run"
    );

    for component in install.components.iter() {
        assert_eq!(
            component.embedded.len(),
            1,
            "expecting each host component to embed one component"
        );

        assert_eq!(
            component.embedded[0].pkg.name(),
            "embedded",
            "expecting the embedded package name to be correct"
        );

        assert_eq!(
            component.embedded[0].components().len(),
            1,
            "expecting the build and run components to get mapped 1:1"
        );

        assert_eq!(
            *component.embedded[0].components().iter().next().unwrap(),
            component.name,
            "expecting the component names to agree"
        );
    }
}

#[rstest]
fn test_embedded_components_extra_components() {
    // If the embedded package has components that the host package doesn't
    // have, they don't get mapped anywhere automatically.
    let install = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
components:
  - name: comp1
embedded:
  - pkg: "embedded/1.0.0/embedded"
    install:
      components:
        - name: comp1
        - name: comp2
        "#,
    )
    .unwrap();

    assert_eq!(
        install.components.len(),
        3,
        "expecting two default components and one explicit component: comp1"
    );

    assert_eq!(install.embedded.len(), 1, "expecting one embedded package");

    assert_eq!(
        install.embedded[0].components().len(),
        4,
        "expecting two default components and two explicit components: comp1 and comp2"
    );

    for component in install.components.iter() {
        assert_eq!(
            component.embedded.len(),
            1,
            "expecting each host component to embed one component"
        );

        assert_eq!(
            component.embedded[0].pkg.name(),
            "embedded",
            "expecting the embedded package name to be correct"
        );

        assert_eq!(
            component.embedded[0].components().len(),
            1,
            "expecting all the host package's components to get mapped 1:1"
        );

        assert_eq!(
            *component.embedded[0].components().iter().next().unwrap(),
            component.name,
            "expecting the component names to agree"
        );

        assert_ne!(
            component.name.as_str(),
            "comp2",
            "expecting the host package to not have comp2"
        );
    }
}

#[rstest]
#[case::comp1("comp1", "1.0.0", &["build", "run"])]
#[case::comp2("comp2", "2.0.0", &["build", "run"])]
#[case::v3_with_all("v3-with-all", "3.0.0", &["build", "run", "aa", "bb", "cc"])]
#[case::v3_with_subset_of_components("v3-with-subset-of-components", "3.0.0", &["aa", "bb"])]
fn test_embedding_multiple_versions_of_the_same_package(
    #[case] component_name: &str,
    #[case] expected_component_version: &str,
    #[case] expected_embedded_components: &[&str],
) {
    // Allow multiple versions of the same package to be embedded. Test that
    // it is possible to assign the different versions to different
    // components in the host package.
    let install = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
components:
  - name: comp1
    embedded:
      - embedded:all/1.0.0/embedded
  - name: comp2
    embedded:
      - embedded:all/2.0.0/embedded
  - name: v3-with-all
    embedded:
      - embedded:all/3.0.0/embedded
  - name: v3-with-subset-of-components
    embedded:
      - embedded:{aa,bb}/3.0.0
embedded:
  - pkg: "embedded/1.0.0/embedded"
  - pkg: "embedded/2.0.0/embedded"
  - pkg: "embedded/3.0.0/embedded"
    install:
      components:
        - name: aa
        - name: bb
        - name: cc
        "#,
    )
    .unwrap();

    assert_eq!(
        install.embedded.len(),
        3,
        "expecting three embedded packages"
    );

    assert_eq!(
        1,
        install
            .embedded
            .iter()
            .map(|p| p.ident().name())
            .collect::<HashSet<_>>()
            .len(),
        "expecting all embedded packages to be the same package"
    );

    assert_eq!(
        3,
        install
            .embedded
            .iter()
            .map(|p| p.ident().version())
            .collect::<HashSet<_>>()
            .len(),
        "expecting the embedded packages to be different versions"
    );

    let comp = install
        .components
        .iter()
        .find(|c| c.name.as_str() == component_name)
        .unwrap();

    assert_eq!(comp.embedded.len(), 1, "expecting one embedded package");

    assert_eq!(
        comp.embedded[0].pkg.target(),
        &Some(Version::from_str(expected_component_version).unwrap()),
        "expecting the embedded package version to be correct"
    );

    assert_eq!(
        expected_embedded_components
            .iter()
            .cloned()
            .collect::<HashSet<_>>(),
        comp.embedded[0]
            .components()
            .iter()
            .map(|c| c.as_str())
            .collect::<HashSet<_>>(),
        "expecting embedded to be expanded correctly"
    );
}

#[rstest]
fn test_fabricated_embedded_not_serialized() {
    // When per-component embedded entries are auto-populated from top-level
    // embedded packages, they should be marked as fabricated and skipped
    // during serialization.
    let install = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
embedded:
  - pkg: "embedded/1.0.0/embedded"
        "#,
    )
    .unwrap();

    // Verify fabricated entries exist at runtime.
    assert!(
        !install.components.is_empty(),
        "expecting default components to be present"
    );
    for component in install.components.iter() {
        assert!(
            !component.embedded.is_empty(),
            "expecting fabricated embedded entries to be populated"
        );
        assert!(
            component.embedded.is_fabricated(),
            "expecting auto-populated embedded entries to be marked as fabricated"
        );
    }

    // Serialize and verify per-component embedded fields are absent.
    let yaml = serde_yaml::to_string(&install).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let components = doc.get("components").expect("expected components key");
    for component in components.as_sequence().unwrap() {
        assert!(
            component.get("embedded").is_none(),
            "fabricated embedded should not appear in serialized output, got: {yaml}"
        );
    }
}

#[rstest]
fn test_explicit_embedded_is_serialized() {
    // When per-component embedded entries are explicitly provided, they
    // should not be marked as fabricated and should be serialized.
    let install = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
components:
  - name: build
    embedded:
      - embedded:build/1.0.0/embedded
  - name: run
    embedded:
      - embedded:run/1.0.0/embedded
embedded:
  - pkg: "embedded/1.0.0/embedded"
        "#,
    )
    .unwrap();

    // Verify entries are not fabricated.
    for component in install.components.iter() {
        assert!(
            !component.embedded.is_fabricated(),
            "expecting explicitly provided embedded entries to not be fabricated"
        );
    }

    // Serialize and verify per-component embedded fields are present.
    let yaml = serde_yaml::to_string(&install).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let components = doc.get("components").expect("expected components key");
    for component in components.as_sequence().unwrap() {
        assert!(
            component.get("embedded").is_some(),
            "explicit embedded should appear in serialized output, got: {yaml}"
        );
    }
}

#[rstest]
fn test_explicit_embedded_roundtrip() {
    // Explicitly provided embedded entries should survive a full
    // serialize-deserialize round-trip.
    let original = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(
        r#"
components:
  - name: build
    embedded:
      - embedded:build/1.0.0/embedded
  - name: run
    embedded:
      - embedded:run/1.0.0/embedded
embedded:
  - pkg: "embedded/1.0.0/embedded"
        "#,
    )
    .unwrap();

    let yaml = serde_yaml::to_string(&original).unwrap();
    let roundtripped = serde_yaml::from_str::<InstallSpec<PinnableRequest>>(&yaml).unwrap();

    assert_eq!(
        original, roundtripped,
        "expected no changes through yaml round-trip"
    );
}
