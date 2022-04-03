// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::parse_version_range;
use crate::{
    api::{parse_version, version_range::Ranged, Spec},
    spec,
};

#[rstest]
fn test_parse_version_range_carat() {
    let vr = parse_version_range("^1.0.1").unwrap();
    assert_eq!(vr.greater_or_equal_to().expect("some version"), "1.0.1");
    assert_eq!(vr.less_than().expect("some version"), "2.0.0");
}

#[rstest]
fn test_parse_version_range_tilde() {
    let vr = parse_version_range("~1.0.1").unwrap();
    assert_eq!(vr.greater_or_equal_to().expect("some version"), "1.0.1");
    assert_eq!(vr.less_than().expect("some version"), "1.1.0");

    assert!(parse_version_range("~2").is_err());
}

#[rstest]
#[case("~1.0.0", "1.0.0", true)]
#[case("~1.0.0", "1.0.1", true)]
#[case("~1.0.0", "1.2.1", false)]
#[case("^1.0.0", "1.0.0", true)]
#[case("^1.0.0", "1.1.0", true)]
#[case("^1.0.0", "1.0.1", true)]
#[case("^1.0.0", "2.0.0", false)]
#[case("^0.1.0", "2.0.0", false)]
#[case("^0.1.0", "0.2.0", false)]
#[case("^0.1.0", "0.1.4", true)]
#[case("1.0.*", "1.0.0", true)]
#[case("1.*", "1.0.0", true)]
#[case("1.*", "1.4.6", true)]
#[case("1.*.0", "1.4.6", false)]
#[case("1.*.0", "1.4.0", true)]
#[case("*", "100.0.0", true)]
#[case(">1.0.0", "1.0.0", false)]
#[case("<1.0.0", "1.0.0", false)]
#[case("<=1.0.0", "1.0.0", true)]
#[case("<=1.0.0", "1.0.1", false)]
#[case(">=1.0.0", "1.0.1", true)]
#[case("1.0.0", "1.0.0", true)]
#[case("1.0.0", "1.0.0", true)]
#[case("!=1.0", "1.0.1", false)]
#[case("!=1.0", "1.1.0", true)]
#[case("=1.0.0", "1.0.0", true)]
#[case("=1.0.0", "1.0.0+r.1", true)]
#[case("=1.0.0+r.2", "1.0.0+r.1", false)]
fn test_version_range_is_applicable(
    #[case] range: &str,
    #[case] version: &str,
    #[case] expected: bool,
) {
    let vr = parse_version_range(range).unwrap();
    let v = parse_version(version).unwrap();
    let actual = vr.is_applicable(&v);

    assert_eq!(actual.is_ok(), expected, "{}", actual);
}

#[rstest]
// exact version compatible with itself: YES
#[allow(clippy::field_reassign_with_default)]
#[case("=1.0.0", spec!({"pkg": "test/1.0.0"}), true)]
// exact version compatible with different post-relese: YES
#[allow(clippy::field_reassign_with_default)]
#[case("=1.0.0", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// exact post release compatible with different one: NO
#[allow(clippy::field_reassign_with_default)]
#[case("=1.0.0+r.2", spec!({"pkg": "test/1.0.0+r.1"}), false)]
// default compat is contextual (given by test function)
#[allow(clippy::field_reassign_with_default)]
#[case("1.0.0", spec!({"pkg": "test/1.1.0/JRSXNRF4", "compat": "x.a.b"}), false)]
// explicit api compat override
#[allow(clippy::field_reassign_with_default)]
#[case("API:1.0.0", spec!({"pkg": "test/1.1.0/JRSXNRF4", "compat": "x.a.b"}), true)]
fn test_version_range_is_satisfied(
    #[case] range: &str,
    #[case] spec: Spec,
    #[case] expected: bool,
) {
    let vr = parse_version_range(range).unwrap();
    let actual = vr.is_satisfied_by(&spec, crate::api::CompatRule::Binary);

    assert_eq!(actual.is_ok(), expected, "{}", actual);
}
