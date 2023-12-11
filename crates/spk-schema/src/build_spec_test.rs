// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spk_schema_foundation::FromYaml;

use super::{BuildSpec, HostCompat};

#[rstest]
fn test_host_compat_default() {
    let host_compat = HostCompat::default();
    assert!(host_compat.is_default());
}

#[rstest]
#[case("Distro", true)]
#[case("Arch", true)]
#[case("Os", true)]
#[case("None", true)]
#[case("Tuesday", false)]
fn test_build_spec_with_host_opt_value(#[case] value: &str, #[case] expected_result: bool) {
    // Tests the host_compat value is valid
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(&format!(
        "{{
        auto_host_vars: {value},
        options: [{{pkg: \"my-pkg\"}}],
    }}"
    ));
    assert!(res.is_ok() == expected_result);
}

#[rstest]
#[case("Distro", vec![String::from("distro"),String::from("arch"),String::from("os")])]
#[case("Arch", vec![String::from("arch"),String::from("os")])]
#[case("Os", vec![String::from("os")])]
#[case("None", vec![])]
fn test_build_spec_with_host_opt_contains_expected_names(
    #[case] value: &str,
    #[case] expected_names: Vec<String>,
) {
    // Test the host_compat value generates the expected named options
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(&format!(
        "{{
        auto_host_vars: {value},
        options: [{{pkg: \"my-pkg\"}}],
    }}"
    ));

    match res {
        Ok(build_spec) => {
            let opt_names: Vec<String> = build_spec
                .opts_for_variant(&build_spec.variants[0])
                .unwrap()
                .iter()
                .map(|o| o.full_name().to_string())
                .collect();
            println!("opt names: {opt_names:?}");

            for name in expected_names.iter() {
                if !opt_names.contains(name) {
                    panic!("Fail: build spec with '{value}' host compat does not contain '{name}'")
                }
            }
        }
        Err(err) => panic!("Fail: {value} host compat in build spec: {err:?}"),
    }
}

#[rstest]
#[case("Distro", vec![])]
#[case("Arch", vec!["distro"])]
#[case("Os", vec!["distro", "arch"])]
#[case("None", vec!["distro", "arch", "os"])]
fn test_build_spec_with_host_opt_does_not_have_disallowed_names(
    #[case] value: &str,
    #[case] invalid_names: Vec<&str>,
) {
    // Test the host_compat value does not create the options names
    // that are disallowed by that value. This test will have to
    // change if the host_compat validation is disabled.
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(&format!(
        "{{
        auto_host_vars: {value},
        options: [{{pkg: \"my-pkg\"}}],
    }}"
    ));

    let unexpected_names = invalid_names
        .iter()
        .map(|s| String::from(*s))
        .collect::<Vec<String>>();

    match res {
        Ok(build_spec) => {
            let opt_names: Vec<String> = build_spec
                .opts_for_variant(&build_spec.variants[0])
                .unwrap()
                .iter()
                .map(|o| o.full_name().to_string())
                .collect();
            println!("opt names: {opt_names:?}");

            for name in unexpected_names.iter() {
                if opt_names.contains(name) {
                    panic!("Fail: build spec with '{value}' host compat does not contain '{name}'")
                }
            }
        }
        Err(err) => panic!("Fail: {value} host compat in build spec: {err:?}"),
    }
}

// #[case("distro")] - not checked, it has no disallowed names
#[rstest]
#[case("Arch")]
#[case("Os")]
#[case("None")]
fn test_build_spec_with_host_opt_and_disallowed_name(#[case] value: &str) {
    // Test the host_compat value setting causes an error when there's
    // a disallowed option name in the build options.
    let res: serde_yaml::Result<BuildSpec> = serde_yaml::from_str(&format!(
        "{{
        auto_host_vars: {value},
        options: [{{var: \"distro/centos\"}}],
    }}"
    ));

    match res {
        Ok(build_spec) => {
            // This return an error because of the "distro/centos" var
            // setting in the variant
            let result = build_spec.opts_for_variant(&build_spec.variants[0]);
            assert!(result.is_ok())
        }
        Err(err) => panic!("Fail: build spec didn't parse with host_compat {value}: {err:?}"),
    }
}

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
  - {opt: a} (GHDY5K3J)
   |     - var: opt
   |   script: echo "hello, world!"
   |   variants:
   |     - opt: a
   |     - opt: a
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
