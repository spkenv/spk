// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema_foundation::FromYaml;
use spk_schema_foundation::version::{
    API_STR,
    BINARY_STR,
    Compatibility,
    InclusionPolicyProblem,
    IncompatibleReason,
};

use super::{InclusionPolicy, PreReleasePolicy, Request};
use crate::parse_build_ident;

#[rstest]
// 1. IncludeAll + ExcludeAll
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Some(PreReleasePolicy::ExcludeAll)
)]
// 2. ExcludeAll + IncludeAll
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Some(PreReleasePolicy::ExcludeAll)
)]
// 3. None + ExcludeAll
#[case(
    "{pkg: something}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Some(PreReleasePolicy::ExcludeAll)
)]
// 4. ExcludeAll + None
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something}",
    Some(PreReleasePolicy::ExcludeAll)
)]
// 5. None + IncludeAll
#[case(
    "{pkg: something}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Some(PreReleasePolicy::IncludeAll)
)]
// 6. IncludeAll + None
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something}",
    Some(PreReleasePolicy::IncludeAll)
)]
// 7. None + None
#[case("{pkg: something}", "{pkg: something}", None)]
// 8. IncludeAll + IncludeAll
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Some(PreReleasePolicy::IncludeAll)
)]
// 9. ExcludeAll + ExcludeAll
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Some(PreReleasePolicy::ExcludeAll)
)]
fn test_prerelease_policy_restricts(
    #[case] request_a: &str,
    #[case] request_b: &str,
    #[case] expected_policy: Option<PreReleasePolicy>,
) {
    let mut a = serde_yaml::from_str::<Request>(request_a)
        .unwrap()
        .pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>(request_b)
        .unwrap()
        .pkg()
        .expect("expected pkg request");

    a.restrict(&b).unwrap();
    assert!(
        expected_policy == a.prerelease_policy,
        "expected restricted prerelease policy to be: {expected_policy:?}"
    )
}

#[rstest]
// 1. IncludeAll > ExcludeAll = Incompatible
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Compatibility::Incompatible(IncompatibleReason::InclusionPolicyNotSuperset(
        InclusionPolicyProblem::Prerelease { our_policy: "IncludeAll".to_string(), other_policy: "ExcludeAll".to_string() }
    ))
)]
// 2. ExcludeAll < IncludeAll
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Compatibility::Compatible
)]
// 3. None == ExcludeAll
#[case(
    "{pkg: something}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Compatibility::Compatible
)]
// 4. ExcludeAll == None
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something}",
    Compatibility::Compatible
)]
// 5. None < IncludeAll
#[case(
    "{pkg: something}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Compatibility::Compatible
)]
// 6. IncludeAll > None = Incompatible
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something}",
    Compatibility::Incompatible(IncompatibleReason::InclusionPolicyNotSuperset(
        InclusionPolicyProblem::Prerelease { our_policy: "IncludeAll".to_string(), other_policy: "None".to_string() }
    ))
)]
// 7. None == None
#[case("{pkg: something}", "{pkg: something}", Compatibility::Compatible)]
// 8.IncludeAll == IncludeAll
#[case(
    "{pkg: something, prereleasePolicy: IncludeAll}",
    "{pkg: something, prereleasePolicy: IncludeAll}",
    Compatibility::Compatible
)]
// 9. ExcludeAll == ExcludeAll
#[case(
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    "{pkg: something, prereleasePolicy: ExcludeAll}",
    Compatibility::Compatible
)]
fn test_prerelease_policy_contains(
    #[case] request_a: &str,
    #[case] request_b: &str,
    #[case] expected_compat: Compatibility,
) {
    let a = serde_yaml::from_str::<Request>(request_a)
        .unwrap()
        .pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>(request_b)
        .unwrap()
        .pkg()
        .expect("expected pkg request");

    let compat = a.contains(&b);
    assert_eq!(expected_compat, compat);
}

#[rstest]
fn test_inclusion_policy() {
    let mut a = serde_yaml::from_str::<Request>("{pkg: something, include: IfAlreadyPresent}")
        .unwrap()
        .pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>("{pkg: something, include: Always}")
        .unwrap()
        .pkg()
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
        .pkg()
        .expect("expected pkg request");
    let b = serde_yaml::from_str::<Request>("{pkg: something/=1.2.3}")
        .unwrap()
        .pkg()
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
    Some(">2.0.0")
)]
// case 2
#[case(
    "{pkg: something/>1.0, include: Always}",
    "{pkg: something/>2.0, include: Always}",
    InclusionPolicy::Always,
    Some(">2.0.0")
)]
// case 3 (mixed)
#[case(
    "{pkg: something/>1.0, include: IfAlreadyPresent}",
    "{pkg: something/>2.0, include: Always}",
    InclusionPolicy::Always,
    Some(">2.0.0")
)]
// case 4 (alt. mixed)
#[case(
    "{pkg: something/>1.0, include: Always}",
    "{pkg: something/>2.0, include: IfAlreadyPresent}",
    InclusionPolicy::Always,
    Some(">2.0.0")
)]
// Two otherwise incompatible requests but are `IfAlreadyPresent`
#[case(
    "{pkg: something/=1.0, include: IfAlreadyPresent}",
    "{pkg: something/=2.0, include: IfAlreadyPresent}",
    InclusionPolicy::IfAlreadyPresent,
    // The requests are merged. This will become an impossible
    // request to satisfy iff a firm request for the package is
    // introduced.
    Some("=1.0.0,=2.0.0")
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
    let mut a = serde_yaml::from_str::<Request>(a).unwrap().pkg().unwrap();
    let b = serde_yaml::from_str::<Request>(b).unwrap().pkg().unwrap();

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
        res.unwrap().var().unwrap().value.is_from_build_env(),
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
#[case::v_expands_into_base_version("1.2.3+r.1", "v", "1.2.3")]
#[case::capital_v_expands_into_full_version("1.2.3+r.1", "V", "1.2.3+r.1")]
#[case::capital_x_in_post_position_expands_all_post_releases(
    "1.2.3+r.1,s.2",
    "x.x+X",
    "1.2+r.1,s.2"
)]
#[case::capital_x_in_post_position_with_no_actual_post_release("1.2.3", "x.x+X", "1.2")]
#[case::capital_x_in_pre_and_post_position_with_no_actual_post_release_expected_order(
    "1.2.3", "x.x-X+X", "1.2"
)]
#[case::capital_x_in_pre_and_post_position_with_no_actual_post_release_unexpected_order(
    "1.2.3", "x.x+X-X", "1.2"
)]
#[case::v_in_post_release_do_not_expand_to_version("1.2.3+v.1", "x.x.x+v.2", "1.2.3+v.2")]
#[should_panic]
#[case::x_in_pre_release_position_is_not_allowed("1.2.3-r.1", "x.x-x", "n/a")]
#[should_panic]
#[case::x_in_post_release_position_is_not_allowed("1.2.3+r.1", "x.x+x", "n/a")]
fn test_pkg_request_pin_rendering(
    #[case] version: &str,
    #[case] pin: &str,
    #[case] expected: &str,
) {
    let req = serde_yaml::from_str::<Request>(&format!("{{pkg: test, fromBuildEnv: {pin}}}"))
        .unwrap()
        .pkg()
        .expect("expected package request");
    let version = parse_build_ident(format!("test/{version}/src")).unwrap();
    let res = req
        .render_pin(&version)
        .expect("should not fail to render pin");
    assert_eq!(format!("{:#}", &res.pkg.version), expected);
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
   |   ^ .[1]: could not determine request type, it may only contain one of the `pkg` or `var` fields at line 2 column 3
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
   |   ^ .[1]: could not determine request type, it must include either a `pkg` or `var` field at line 2 column 3
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
   | ^ request for `option` cannot specify a value `/my-value` when `fromBuildEnv` is true
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
   | ^ request for `python` cannot specify a value `/3` when `fromBuildEnv` is specified
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
        .map(|r| r.pkg().expect("expected a pkg request").pin)
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
