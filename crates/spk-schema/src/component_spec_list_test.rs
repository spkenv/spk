// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::ComponentSpecList;
use crate::foundation::ident_component::Component;
use crate::v0;

#[rstest]
fn test_components_no_duplicates() {
    serde_yaml::from_str::<ComponentSpecList<v0::Package>>("[{name: python}, {name: other}]")
        .expect("should succeed in a simple case with two components");
    serde_yaml::from_str::<ComponentSpecList<v0::Package>>("[{name: python}, {name: python}]")
        .expect_err("should fail to deserialize with the same component twice");
}

#[rstest]
fn test_components_has_defaults() {
    let components = serde_yaml::from_str::<ComponentSpecList<v0::Package>>("[]").unwrap();
    assert_eq!(components.len(), 2, "Should receive default components");
    let components =
        serde_yaml::from_str::<ComponentSpecList<v0::Package>>("[{name: run}, {name: build}]")
            .unwrap();
    assert_eq!(components.len(), 2, "Should not receive default components");
}

#[rstest]
fn test_components_uses_must_exist() {
    serde_yaml::from_str::<ComponentSpecList<v0::Package>>(
        "[{name: python, uses: [other]}, {name: other}]",
    )
    .expect("should succeed in a simple case");
    serde_yaml::from_str::<ComponentSpecList<v0::Package>>("[{name: python, uses: [other]}]")
        .expect_err("should fail when the used component does not exist");
}

#[rstest]
fn test_resolve_uses() {
    let components = serde_yaml::from_str::<ComponentSpecList<v0::Package>>(
        r#"[
                {name: build, uses: [dev, libstatic]},
                {name: run, uses: [bin, lib]},
                {name: bin, uses: [lib]},
                {name: dev, uses: [lib]},
                {name: lib},
                {name: libstatic},
                ]"#,
    )
    .unwrap();
    let actual = components.resolve_uses([Component::Build].iter());
    let expected = vec!["build", "dev", "libstatic", "lib"]
        .into_iter()
        .map(Component::parse)
        .map(Result::unwrap)
        .collect();
    assert_eq!(actual, expected);
}

#[rstest]
fn test_resolve_uses_all() {
    let components = serde_yaml::from_str::<ComponentSpecList<v0::Package>>(
        r#"[
                {name: build, uses: [dev, libstatic]},
                {name: run, uses: [bin, lib]},
                {name: bin, uses: [lib]},
                {name: dev, uses: [lib]},
                {name: lib},
                {name: libstatic},
                ]"#,
    )
    .unwrap();
    let actual = components.resolve_uses([Component::All].iter());
    let expected = vec!["build", "dev", "libstatic", "lib", "run", "bin"]
        .into_iter()
        .map(Component::parse)
        .map(Result::unwrap)
        .collect();
    assert_eq!(actual, expected);
}
