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
#[case("==1.0.0", "1.0.0+r.1", false)]
#[case("=1.0.0+r.2", "1.0.0+r.1", false)]
fn test_version_range_is_applicable(
    #[case] range: &str,
    #[case] version: &str,
    #[case] expected: bool,
) {
    let vr = parse_version_range(range).unwrap();
    let v = parse_version(version).unwrap();
    let actual = vr.is_applicable(&v);

    assert_eq!(
        actual.is_ok(),
        expected,
        "\"{}\".is_applicable({}) {}",
        range,
        version,
        actual
    );
}

#[rstest]
// exact version compatible with itself: YES
#[case("=1.0.0", spec!({"pkg": "test/1.0.0"}), true)]
// shorter parts version compatible with itself: YES
#[case("=1.0", spec!({"pkg": "test/1.0.0"}), true)]
// exact version compatible with different post-release: YES
#[case("=1.0.0", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// shorter parts version compatible with different post-release: YES
#[case("=1.0", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// precise exact version compatible with different post-release: NO
#[case("==1.0.0", spec!({"pkg": "test/1.0.0+r.1"}), false)]
// precise shorter parts version compatible with same post-release: YES
#[case("==1.0+r.1", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// exact post release compatible with different one: NO
#[case("=1.0.0+r.2", spec!({"pkg": "test/1.0.0+r.1"}), false)]
// negative exact version compatible with itself: NO
#[case("!=1.0.0", spec!({"pkg": "test/1.0.0"}), false)]
// negative shorter parts version compatible with itself: NO
#[case("!=1.0", spec!({"pkg": "test/1.0.0"}), false)]
// negative exact version compatible with different post-release: NO
#[case("!=1.0.0", spec!({"pkg": "test/1.0.0+r.1"}), false)]
// negative precise exact version compatible with different post-release: YES
#[case("!==1.0.0", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// negative precise shorter parts version compatible with different post-release: YES
#[case("!==1.0", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// negative precise shorter parts version compatible with same post-release: NO
#[case("!==1.0+r.1", spec!({"pkg": "test/1.0.0+r.1"}), false)]
// negative exact post release compatible with different one: YES
#[case("!=1.0.0+r.2", spec!({"pkg": "test/1.0.0+r.1"}), true)]
// default compat is contextual (given by test function)
#[case("1.0.0", spec!({"pkg": "test/1.1.0/JRSXNRF4", "compat": "x.a.b"}), false)]
// explicit api compat override
#[case("API:1.0.0", spec!({"pkg": "test/1.1.0/JRSXNRF4", "compat": "x.a.b"}), true)]
// unspecified parts in request have no opinion (rather than requesting zero)
#[case("1", spec!({"pkg": "test/1.2.3/JRSXNRF4", "compat": "x.a.b"}), true)]
// newer post-release but `x.x.x` compat with API compatibility
#[case("API:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x"}), true)]
// newer post-release but `x.x.x` compat with Binary compatibility
#[case("Binary:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x"}), true)]
// newer post-release but `x.x.x+x` compat with API compatibility
#[case("API:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+x"}), false)]
// newer post-release but `x.x.x+x` compat with Binary compatibility
#[case("Binary:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+x"}), false)]
// newer post-release but `x.x.x+a` compat with API compatibility
#[case("API:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+a"}), true)]
// newer post-release but `x.x.x+a` compat with Binary compatibility
#[case("Binary:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+a"}), false)]
// newer post-release but `x.x.x+b` compat with API compatibility
#[case("API:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+b"}), false)]
// newer post-release but `x.x.x+b` compat with Binary compatibility
#[case("Binary:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+b"}), true)]
// newer post-release but `x.x.x+ab` compat with API compatibility
#[case("API:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+ab"}), true)]
// newer post-release but `x.x.x+ab` compat with Binary compatibility
#[case("Binary:1.38.0", spec!({"pkg": "test/1.38.0+r.3/JRSXNRF4", "compat": "x.x.x+ab"}), true)]
fn test_version_range_is_satisfied(
    #[case] range: &str,
    #[case] spec: Spec,
    #[case] expected: bool,
) {
    let vr = parse_version_range(range).unwrap();
    let actual = vr.is_satisfied_by(&spec, crate::api::CompatRule::Binary);

    assert_eq!(actual.is_ok(), expected, "{} -> {:?}", range, actual);
}
