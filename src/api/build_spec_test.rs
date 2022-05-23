// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::BuildSpec;
use crate::{api, option_map};

#[rstest]
fn test_variants_must_be_unique() {
    // two variants end up resolving to the same set of options
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(
        r#"{
        options: [{var: "my-opt/any-value"}],
        variants: [{my-opt: "any-value"}, {}],
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

#[rstest]
fn test_resolve_all_options_package_option() {
    let spec: BuildSpec = serde_yaml::from_str(
        r#"{
            options: [
                {var: "python.abi/cp37m"},
                {var: "my-opt/default"},
                {var: "debug/off"},
            ]
        }"#,
    )
    .unwrap();

    let options = option_map! {
        "python.abi" => "cp27mu",
        "my-opt" => "value",
        "my-pkg.my-opt" => "override",
        "debug" => "on",
    };
    let name: api::PkgNameBuf = "my-pkg".parse().unwrap();
    let resolved = spec.resolve_all_options(Some(&name), &options);
    assert_eq!(
        resolved.get("my-opt"),
        Some(&"override".to_string()),
        "namespaced option should take precedence"
    );
    assert_eq!(
        resolved.get("debug"),
        Some(&"on".to_string()),
        "global opt should resolve if given"
    );
    assert_eq!(
        resolved.get("python.abi"),
        Some(&"cp27mu".to_string()),
        "opt for other package should exist"
    );
}
