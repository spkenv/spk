// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::ComponentSpecList;

#[rstest]
fn test_components_no_duplicates() {
    serde_yaml::from_str::<ComponentSpecList>("[{name: python}, {name: other}]")
        .expect("should succeed in a simple case with two components");
    serde_yaml::from_str::<ComponentSpecList>("[{name: python}, {name: python}]")
        .expect_err("should fail to deserialize with the same component twice");
}

#[rstest]
fn test_components_has_defaults() {
    let components = serde_yaml::from_str::<ComponentSpecList>("[]").unwrap();
    assert_eq!(components.len(), 2, "Should receive default components");
    let components =
        serde_yaml::from_str::<ComponentSpecList>("[{name: run}, {name: build}]").unwrap();
    assert_eq!(components.len(), 2, "Should not receive default components");
}

#[rstest]
fn test_components_uses_must_exist() {
    serde_yaml::from_str::<ComponentSpecList>("[{name: python, uses: [other]}, {name: other}]")
        .expect("should succeed in a simple case");
    serde_yaml::from_str::<ComponentSpecList>("[{name: python, uses: [other]}]")
        .expect_err("should fail when the used component does not exist");
}
