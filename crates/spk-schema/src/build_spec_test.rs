// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::BuildSpec;

#[rstest]
fn test_variants_may_have_a_build() {
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(
        r#"{
        options: [{pkg: "my-pkg"}],
        variants: [{my-pkg: "1.0.0/QYB6QLCN"}],
    }"#,
    );

    assert!(res.is_ok());
}

#[rstest]
fn test_variants_must_be_unique() {
    // two variants end up resolving to the same set of options
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(
        r#"{
        variants: [{my-opt: "any-value"}, {my-opt: "any-value"}],
    }"#,
    );

    assert!(res.is_err());
}

#[rstest]
fn test_variants_must_be_unique_unknown_ok() {
    // unrecognized variant values are ok if they are unique still
    let _: BuildSpec =
        serde_yaml::from_str("{variants: [{unknown: any-value}, {unknown: any_other_value}]}")
            .unwrap();
}
