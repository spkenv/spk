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
    assert_eq!(vr.greater_or_equal_to(), Some("1.0.1".parse().unwrap()));
    assert_eq!(vr.less_than(), Some("2.0.0".parse().unwrap()));
}

#[rstest]
fn test_parse_version_range_tilde() {
    let vr = parse_version_range("~1.0.1").unwrap();
    assert_eq!(vr.greater_or_equal_to().unwrap(), "1.0.1".parse().unwrap());
    assert_eq!(vr.less_than().unwrap(), "1.1.0".parse().unwrap());

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
#[case("=1.0.0", spec!{pkg => "test/1.0.0"}, true)]
#[case("=1.0.0", spec!{pkg => "test/1.0.0+r.1"}, true)]
#[case("=1.0.0+r.2", spec!{pkg => "test/1.0.0+r.1"}, false)]
fn test_version_range_is_satisfied(
    #[case] range: &str,
    #[case] spec: Spec,
    #[case] expected: bool,
) {
    let vr = parse_version_range(range).unwrap();
    let actual = vr.is_satisfied_by(&spec, crate::api::CompatRule::Binary);

    assert_eq!(actual.is_ok(), expected, "{}", actual);
}
