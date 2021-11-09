// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::InstallSpec;

#[rstest]
fn test_components_no_duplicates() {
    serde_yaml::from_str::<InstallSpec>("components: [{name: python}, {name: other}]")
        .expect("should succeed in a simple case with two components");
    serde_yaml::from_str::<InstallSpec>("components: [{name: python}, {name: python}]")
        .expect_err("should fail to deserialize with the same component twice");
}

#[rstest]
fn test_components_uses_must_exist() {
    serde_yaml::from_str::<InstallSpec>(
        "components: [{name: python, uses: [other]}, {name: other}]",
    )
    .expect("should succeed in a simple case");
    serde_yaml::from_str::<InstallSpec>("components: [{name: python, uses: [other]}]")
        .expect_err("should fail when the used component does not exist");
}
