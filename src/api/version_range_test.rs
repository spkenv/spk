// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use proptest::{
    collection::{btree_map, vec},
    option::weighted,
    prelude::*,
};

use super::{
    parse_version_range, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    NotEqualsVersion, WildcardRange,
};
use crate::{
    api::{
        parse_version, version::VersionParts, version_range::Ranged, CompatRange, CompatRule, Spec,
        TagSet, Version, VersionRange,
    },
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

#[rstest]
#[case("!=1.2.0", "=1.1.9", true)]
#[case("!=1.2.0", "=1.2.0", false)]
#[case("!=1.2.0", "=1.2.1", true)]
#[case("<1.2", "<1.2", true)]
#[case("<1.2", "<1.3", true)]
#[case("<1.2", "=1.1", true)]
#[case("<1.2", "=1.2", false)]
#[case("<1.2", "=1.2.0.1", false)]
#[case("<1.2", "=1.3", false)]
#[case("<=1.2", "=1.2", true)]
#[case("<=1.2", "=1.3", false)]
#[case("=1.2", "=1.1", false)]
#[case(">1.0", "<2.0", true)]
#[case(">1.0", "=1.0", false)]
#[case(">1.0", "=2.0", true)]
#[case(">1.0", ">2.0", true)]
#[case(">=1.0", "=1.0", true)]
#[case(">=1.0", ">=2.0", true)]
#[case(">=1.2", "=1.1", false)]
#[case("~1.2.0", "=1.2.1", true)]
fn test_intersects(#[case] range1: &str, #[case] range2: &str, #[case] expected: bool) {
    let a = parse_version_range(range1).unwrap();
    let b = parse_version_range(range2).unwrap();
    let c = a.intersects(&b);
    assert_eq!(!&c, !expected, "a:{} + b:{} == {:?}", a, b, c);
    let c = b.intersects(&a);
    assert_eq!(!&c, !expected, "b:{} + a:{} == {:?}", b, a, c);
}

prop_compose! {
    // XXX: The tagset is limited to a maximum of one entry because of
    // the ambiguous use of commas to delimit both tags and version filters.
    fn arb_tagset()(tags in btree_map("[a-zA-Z0-9]+", any::<u32>(), 0..=1)) -> TagSet {
        TagSet { tags }
    }
}

fn arb_version() -> impl Strategy<Value = Version> {
    arb_version_min_len(1)
}

fn arb_version_min_len(min_len: usize) -> impl Strategy<Value = Version> {
    (
        vec(any::<u32>(), min_len..min_len.max(10)),
        arb_tagset(),
        arb_tagset(),
    )
        .prop_map(|(parts, pre, post)| Version {
            parts: VersionParts {
                parts,
                plus_epsilon: false,
            },
            pre,
            post,
        })
}

prop_compose! {
    // CompatRule::None intentionally not included in this list.
    fn arb_compat_rule()(cr in prop_oneof![Just(CompatRule::API), Just(CompatRule::Binary)]) -> CompatRule {
        cr
    }
}

fn arb_wildcard_range_from_version(version: Version) -> impl Strategy<Value = VersionRange> {
    (Just(version.clone()), 0..version.parts.len()).prop_map(|(version, index_to_wildcard)| {
        VersionRange::Wildcard(WildcardRange {
            specified: version.parts.len(),
            parts: version
                .parts
                .iter()
                .enumerate()
                .map(|(index, num)| {
                    if index == index_to_wildcard {
                        None
                    } else {
                        Some(*num)
                    }
                })
                .collect::<Vec<_>>(),
        })
    })
}

fn arb_range_that_includes_version(version: Version) -> impl Strategy<Value = VersionRange> {
    prop_oneof![
        // Compat: the same version
        (weighted(0.33, arb_compat_rule()), Just(version.clone()))
            .prop_map(|(required, base)| { VersionRange::Compat(CompatRange { base, required }) }),
        // DoubleEquals: the same version
        Just(VersionRange::DoubleEquals(DoubleEqualsVersion {
            version: version.clone()
        })),
        // DoubleNotEquals: an arbitrary version that isn't equal
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "not the same version",
                |(other_version, version)| other_version != version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::DoubleNotEquals(DoubleNotEqualsVersion {
                    specified: other_version.parts.len(),
                    base: other_version,
                })
            }),
        // Equals: the same version
        Just(VersionRange::Equals(EqualsVersion {
            version: version.clone()
        })),
        // Filter: skipping for now
        // GreaterThan: an arbitrary version that is <=
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "a less than or equal version",
                |(other_version, version)| other_version <= version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::GreaterThan(GreaterThanRange {
                    bound: other_version,
                })
            }),
        // GreaterThanOrEqualTo: an arbitrary version that is <
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "a less than version",
                |(other_version, version)| other_version < version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::GreaterThanOrEqualTo(GreaterThanOrEqualToRange {
                    bound: other_version,
                })
            }),
        // LesserThan: an arbitrary version that is >=
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "a greater than or equal version",
                |(other_version, version)| other_version >= version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::LessThan(LessThanRange {
                    bound: other_version,
                })
            }),
        // LessThanOrEqualTo: an arbitrary version that is >
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "a greater than version",
                |(other_version, version)| other_version > version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::LessThanOrEqualTo(LessThanOrEqualToRange {
                    bound: other_version,
                })
            }),
        // LowestSpecified: skipping for now
        // NotEquals: an arbitrary version that isn't equal
        (arb_version(), Just(version.clone()))
            .prop_filter(
                "not the same version",
                |(other_version, version)| other_version != version
            )
            .prop_map(|(other_version, _)| {
                VersionRange::NotEquals(NotEqualsVersion {
                    specified: other_version.parts.len(),
                    base: other_version,
                })
            }),
        // Semver: skipping for now
        // Wildcard: turn one of the version digits into a wildcard
        arb_wildcard_range_from_version(version),
    ]
}

fn arb_pair_of_intersecting_ranges() -> impl Strategy<Value = (Version, VersionRange, VersionRange)>
{
    arb_version().prop_flat_map(|version| {
        (
            arb_range_that_includes_version(version.clone()),
            arb_range_that_includes_version(version.clone()),
        )
            .prop_map(move |(r1, r2)| (version.clone(), r1, r2))
    })
}

proptest! {
    /// Generate an arbitrary version and two arbitrary ranges
    /// that the version belongs to. Therefore, the two ranges
    /// are expected to intersect.
    #[test]
    fn prop_test_range_intersect(
            pair in arb_pair_of_intersecting_ranges()) {
        let (version, a, b) = pair;
        let c = a.intersects(&b);
        prop_assert!(c.is_ok(), "{} -- a:{} + b:{} == {:?}", version, a, b, c);
        let c = b.intersects(&a);
        prop_assert!(c.is_ok(), "{} -- b:{} + a:{} == {:?}", version, b, a, c);
    }
}
