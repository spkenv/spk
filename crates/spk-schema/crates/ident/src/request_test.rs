// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema_foundation::version::{API_STR, BINARY_STR};
use spk_schema_foundation::FromYaml;

use super::{InclusionPolicy, PreReleasePolicy, Request};
use crate::parse_ident;

#[rstest]
fn test_prerelease_policy() {
    let mut a = serde_yaml::from_str::<Request>("{pkg: something, prereleasePolicy: IncludeAll}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>("{pkg: something, prereleasePolicy: ExcludeAll}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");

    a.restrict(&b).unwrap();
    match a.prerelease_policy {
        PreReleasePolicy::ExcludeAll => (),
        _ => panic!("expected restricted prerelease policy"),
    }
}

#[rstest]
fn test_inclusion_policy() {
    let mut a = serde_yaml::from_str::<Request>("{pkg: something, include: IfAlreadyPresent}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>("{pkg: something, include: Always}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");

    a.restrict(&b).unwrap();
    match a.inclusion_policy {
        InclusionPolicy::Always => (),
        _ => panic!("expected restricted inclusion policy"),
    }
}

#[rstest]
fn test_compat_and_equals_restrict() {
    let mut a = serde_yaml::from_str::<Request>("{pkg: something/Binary:1.2.3}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>("{pkg: something/=1.2.3}")
        .unwrap()
        .into_pkg()
        .expect("expected pkg request");

    a.restrict(&b).unwrap();
    assert_eq!(a.pkg.version.to_string(), "=1.2.3");
}

#[rstest]
// Compatible inclusion policies are expected to merge,
// case 1
#[case(
    "{pkg: something/>1.0, include: IfAlreadyPresent}",
    "{pkg: something/>2.0, include: IfAlreadyPresent}",
    InclusionPolicy::IfAlreadyPresent,
    Some(">2.0")
)]
// case 2
#[case(
    "{pkg: something/>1.0, include: Always}",
    "{pkg: something/>2.0, include: Always}",
    InclusionPolicy::Always,
    Some(">2.0")
)]
// case 3 (mixed)
#[case(
    "{pkg: something/>1.0, include: IfAlreadyPresent}",
    "{pkg: something/>2.0, include: Always}",
    InclusionPolicy::Always,
    Some(">2.0")
)]
// case 4 (alt. mixed)
#[case(
    "{pkg: something/>1.0, include: Always}",
    "{pkg: something/>2.0, include: IfAlreadyPresent}",
    InclusionPolicy::Always,
    Some(">2.0")
)]
// Two otherwise incompatible requests but are `IfAlreadyPresent`
#[case(
    "{pkg: something/=1.0, include: IfAlreadyPresent}",
    "{pkg: something/=2.0, include: IfAlreadyPresent}",
    InclusionPolicy::IfAlreadyPresent,
    // The requests are merged. This will become an impossible
    // request to satisfy iff a firm request for the package is
    // introduced.
    Some("=1.0,=2.0")
)]
// Incompatible requests when something is `Always` is a restrict
// failure.
#[case(
    "{pkg: something/=1.0, include: IfAlreadyPresent}",
    "{pkg: something/=2.0, include: Always}",
    InclusionPolicy::Always,
    None
)]
fn test_inclusion_policy_and_merge(
    #[case] a: &str,
    #[case] b: &str,
    #[case] expected_policy: InclusionPolicy,
    #[case] expected_merged_range: Option<&str>,
) {
    let mut a = serde_yaml::from_str::<Request>(a)
        .unwrap()
        .into_pkg()
        .unwrap();
    let b = serde_yaml::from_str::<Request>(b)
        .unwrap()
        .into_pkg()
        .unwrap();

    let r = a.restrict(&b);
    match expected_merged_range {
        Some(expected_merged_range) => {
            assert!(r.is_ok());
            assert_eq!(a.inclusion_policy, expected_policy);
            assert_eq!(a.pkg.version.to_string().as_str(), expected_merged_range);
        }
        None => {
            assert_eq!(a.inclusion_policy, expected_policy);
            assert!(r.is_err());
        }
    }
}

#[rstest]
fn test_deserialize_value_or_pin() {
    let res = serde_yaml::from_str::<Request>("{var: python.abi/cp27m}");
    assert!(res.is_ok(), "should allow regular name/value");

    let res = serde_yaml::from_str::<Request>("{var: python.abi, fromBuildEnv: true}");
    assert!(res.is_ok(), "should allow no value when pinning build env");

    let res = serde_yaml::from_str::<Request>("{var: python.abi/cp27m, fromBuildEnv: true}");
    assert!(res.is_err(), "should not allow value and pin");

    let res = serde_yaml::from_str::<Request>("{var: python.abi}");
    assert!(res.is_err(), "should not allow omitting value without pin");
}

#[rstest]
fn test_var_request_empty_value_roundtrip() {
    let req = serde_yaml::from_str::<Request>("{var: python.abi/}").unwrap();
    let yaml = serde_yaml::to_string(&req).unwrap();
    let res = serde_yaml::from_str::<Request>(&yaml);
    assert!(
        res.is_ok(),
        "should be able to round-trip serialize a var request with empty string value"
    );
}

#[rstest]
fn test_var_request_pinned_roundtrip() {
    let req = serde_yaml::from_str::<Request>("{var: python.abi, fromBuildEnv: true}").unwrap();
    let yaml = serde_yaml::to_string(&req).unwrap();
    let res = serde_yaml::from_str::<Request>(&yaml);
    assert!(
        res.is_ok(),
        "should be able to round-trip serialize a var request with pin"
    );
    assert!(
        res.unwrap().into_var().unwrap().pin,
        "should preserve pin value"
    );
}

#[rstest]
#[case("1.2.3", "x.x.x", "1.2.3")]
#[case("1.2.3", "x", "1")]
#[case("1.2.3", "x.x", "1.2")]
#[case("1.2.3", "~x.x.x.x", "~1.2.3.0")]
#[case("1.2.3", "Binary", "Binary:1.2.3")]
#[case("1.2.3", "API", "API:1.2.3")]
#[case("1.2.3.4.5", "API", "API:1.2.3.4.5")]
#[case("1.2.3", "API:x.x", "API:1.2")]
#[case("1.2.3", "true", "Binary:1.2.3")]
fn test_pkg_request_pin_rendering(
    #[case] version: &str,
    #[case] pin: &str,
    #[case] expected: &str,
) {
    let req = serde_yaml::from_str::<Request>(&format!("{{pkg: test, fromBuildEnv: {}}}", pin))
        .unwrap()
        .into_pkg()
        .expect("expected package request");
    let version = parse_ident(format!("test/{}", version)).unwrap();
    let res = req
        .render_pin(&version)
        .expect("should not fail to render pin");
    assert_eq!(&res.pkg.version.to_string(), expected);
}

/// Confirm that the error provided when both 'var' or 'pkg' field
/// exist is meaningful and positioned reasonably
#[rstest]
fn test_yaml_error_ambiguous() {
    format_serde_error::never_color();
    // use a vector of requests just to confirm the error positioning
    static YAML: &str = r#"- var: os/linux
- var: hello
  pkg: hello
"#;
    let err = Vec::<Request>::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
   | - var: os/linux
 2 | - var: hello
   |      ^ .[1]: could not determine request type, it may only contain one of the `pkg` or `var` fields at line 2 column 6
   |   pkg: hello
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that the error provided when no 'var' or 'pkg' field
/// exists is meaningful and positioned reasonably
#[rstest]
fn test_yaml_error_undetermined() {
    format_serde_error::never_color();
    // use a vector of requests just to confirm the error positioning
    static YAML: &str = r#"- var: os/linux
- pin: true
  value: default
"#;
    let err = Vec::<Request>::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
   | - var: os/linux
 2 | - pin: true
   |      ^ .[1]: could not determine request type, it must include either a `pkg` or `var` field at line 2 column 6
   |   value: default
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that the deserialize error is useful
/// when both a value and fromBuildEnv are given
#[rstest]
fn test_yaml_error_var_value_and_pin() {
    format_serde_error::never_color();
    static YAML: &str = r#"var: option/my-value
fromBuildEnv: true
"#;
    let err = Request::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
 1 | var: option/my-value
   |    ^ request for `option` cannot specify a value `/my-value` when `fromBuildEnv` is true at line 1 column 4
   | fromBuildEnv: true
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that the deserialize error is useful
/// when both a version and fromBuildEnv are given
#[rstest]
fn test_yaml_error_pkg_version_and_pin() {
    format_serde_error::never_color();
    static YAML: &str = r#"pkg: python/3
fromBuildEnv: true
"#;
    let err = Request::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
 1 | pkg: python/3
   |    ^ request for `python` cannot specify a value `/3` when `fromBuildEnv` is specified at line 1 column 4
   | fromBuildEnv: true
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that an invalid package name points
/// to the actual package name position in the yaml error
#[rstest]
fn test_yaml_error_invalid_name_position() {
    format_serde_error::never_color();
    static YAML: &str = r#"{
    fromBuildEnv: true,
    pkg: pytHon
}"#;
    let err = Request::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
   | {
   |     fromBuildEnv: true,
 3 |     pkg: pytHon
   |          ^ pkg: Error: expected eof at Hon at line 3 column 10
   | }
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that booleans and strings are valid for
/// the pkg fromBuildEnv field
#[rstest]
fn test_deserialize_pkg_pin_string_or_bool() {
    format_serde_error::never_color();
    static YAML: &str = r#"
- pkg: python
  fromBuildEnv: true
- pkg: python
  fromBuildEnv: x.x
- pkg: python
  fromBuildEnv: Binary
- pkg: python
  fromBuildEnv: API
"#;
    let reqs = Vec::<Request>::from_yaml(YAML).expect("expected yaml parsing to succeed");
    let pins: Vec<_> = reqs
        .into_iter()
        .map(|r| r.into_pkg().expect("expected a pkg request").pin)
        .collect();
    assert_eq!(
        pins,
        vec![
            Some(String::from(BINARY_STR)),
            Some(String::from("x.x")),
            Some(String::from(BINARY_STR)),
            Some(String::from(API_STR))
        ]
    );
}
