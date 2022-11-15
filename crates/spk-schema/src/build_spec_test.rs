// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pretty_assertions::assert_eq;
use rstest::rstest;
use spk_schema_foundation::FromYaml;

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

/// Confirm that there is a reasonable yaml representation
/// for errors that occur while converting the unchecked build
/// spec into a checked one.
#[rstest]
fn test_yaml_error_unchecked_to_checked() {
    format_serde_error::never_color();
    static YAML: &str = r#"- options:
    - var: opt
  script: echo "hello, world!"
  variants:
    - opt: a
    - opt: a
"#;
    let err = Vec::<BuildSpec>::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
 1 | - options:
   | ^ Error: Multiple variants would produce the same build:
  - {opt: a} (GHDY5K3J)
  - {opt: a} (GHDY5K3J) at line 1 column 1
   |     - var: opt
   |   script: echo "hello, world!"
   |   variants:
   |     - opt: a
   |     - opt: a
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
