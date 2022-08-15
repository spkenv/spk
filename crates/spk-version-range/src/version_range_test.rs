// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use proptest::{
    collection::{btree_map, vec},
    option::weighted,
    prelude::*,
};
use rstest::rstest;
use spk_spec::{spec, Spec};
use spk_version::{parse_version, CompatRule, TagSet, Version, VersionParts};

use super::{
    parse_version_range, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, SemverRange, WildcardRange,
};
use crate::{CompatRange, Ranged, VersionRange};

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
    let actual = vr.is_satisfied_by(&spec, CompatRule::Binary);

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
#[case("1.73.0+r.2", "=1.73.0", true)]
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
    // Generate a minimum of two version elements to accommodate
    // `LowestSpecifiedRange`'s requirements.
    arb_version_min_len(2)
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

fn arb_lowest_specified_range_from_version(
    version: Version,
) -> impl Strategy<Value = VersionRange> {
    // A version like 1.2.3 will be valid in the following expressions:
    //   - ~1.0
    //   - ~1.1
    //   - ~1.2
    //   - ~1.2.0
    //   - ~1.2.1
    //   - ~1.2.2
    //   - ~1.2.3
    //
    // The numbers have to match the original numbers, except the last
    // element specified, which may be <= the origin number
    (Just(version.clone()), 2..=version.parts.len()).prop_flat_map(
        |(version, parts_to_generate)| {
            // Generate what number to use in the last position.
            (
                Just(version.clone()),
                Just(parts_to_generate),
                0..=(*version.parts.get(parts_to_generate - 1).unwrap()),
            )
                .prop_map(|(version, parts_to_generate, last_element_value)| {
                    VersionRange::LowestSpecified(LowestSpecifiedRange {
                        specified: parts_to_generate,
                        base: Version {
                            parts: version
                                .parts
                                .iter()
                                .take(parts_to_generate)
                                .enumerate()
                                .map(|(index, num)| {
                                    if index == parts_to_generate - 1 {
                                        last_element_value
                                    } else {
                                        *num
                                    }
                                })
                                .collect::<Vec<_>>()
                                .into(),
                            // Retain pre and post from original version because
                            // if the original has pre it might be smaller than
                            // the smallest value we generated without it.
                            pre: version.pre,
                            post: version.post,
                        },
                    })
                })
        },
    )
}

fn arb_semver_range_from_version(version: Version) -> impl Strategy<Value = VersionRange> {
    // A version like 2.3.4 will be valid in the following expressions:
    //   - ^2
    //   - ^2.0
    //   - ^2.1
    //   - ^2.2
    //   - ^2.3
    //   - ^2.0.0
    //   - ^2.1.0
    //   - ^2.2.0
    //   - ^2.3.0
    //   - ^2.3.1
    //   - ^2.3.2
    //   - ^2.3.3
    //   - ^2.3.4
    //
    // The left-most non-zero digit must match the original number,
    // and the remaining elements must be <= the original numbers.
    (Just(version.clone()), 1..=version.parts.len()).prop_flat_map(
        |(version, parts_to_generate)| {
            // Generate what numbers to use in each position.
            let ranges = version
                .parts
                .iter()
                .take(parts_to_generate)
                .map(|num| 0..=*num)
                .collect::<Vec<_>>();

            (Just(version), Just(parts_to_generate), ranges).prop_map(
                |(version, parts_to_generate, values_to_use)| {
                    let mut found_non_zero = false;
                    VersionRange::Semver(SemverRange {
                        minimum: Version {
                            parts: version
                                .parts
                                .iter()
                                .take(parts_to_generate)
                                .zip(values_to_use.iter())
                                .map(|(actual_number, proposed_number)| {
                                    if !found_non_zero && *actual_number == 0 {
                                        found_non_zero = true;
                                        *actual_number
                                    } else if !found_non_zero {
                                        *actual_number
                                    } else {
                                        *proposed_number
                                    }
                                })
                                .collect::<Vec<_>>()
                                .into(),
                            // Retain pre and post from original version because
                            // if the original has pre it might be smaller than
                            // the smallest value we generated without it.
                            pre: version.pre,
                            post: version.post,
                        },
                    })
                },
            )
        },
    )
}

fn arb_wildcard_range_from_version(version: Version) -> impl Strategy<Value = VersionRange> {
    (Just(version.clone()), 0..version.parts.len()).prop_map(|(version, index_to_wildcard)| {
        debug_assert!(!version.parts.is_empty());
        let parts = version
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
            .collect::<Vec<_>>();
        VersionRange::Wildcard(
            // Safety: we generate parts with the required one and only one
            // optional element.
            unsafe { WildcardRange::new_unchecked(parts) },
        )
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
        // LowestSpecified: transform version digits
        arb_lowest_specified_range_from_version(version.clone()),
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
        // Semver: transform version digits
        arb_semver_range_from_version(version.clone()),
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

#[rstest]
#[case("<1.0", "<2.0", false)]
#[case("<1.0", "<=2.0", false)]
#[case("<1.0", "=2.0", false)]
#[case("<2.0", "<1.0", true)]
#[case("<2.0", "=1.0", true)]
#[case("<3.0", ">2.0", false)]
#[case("<3.0", ">=2.0", false)]
#[case("<=1.0", "<2.0", false)]
#[case("<=1.0", "<=2.0", false)]
#[case("<=2.0", "=2.0", true)]
#[case("<=3.0", ">2.0", false)]
#[case("<=3.0", ">=2.0", false)]
#[case("=1.0", "=1.0", true)]
#[case(">1.0", "=2.0", true)]
#[case(">1.0", ">2.0", true)]
#[case(">2.0", "<1.0", false)]
#[case(">2.0", "<3.0", false)]
#[case(">2.0", "=1.0", false)]
#[case(">2.0", ">1.0", false)]
#[case(">=1.0", "=1.0", true)]
#[case(">=1.0,<=3.0", "=2.0", true)]
#[case("~1.2.3", ">1.2", false)]
#[case("~1.2.3", ">1.2.4", false)]
#[case("API:1.2.3", "Binary:1.2.3", true)] // increasing strictness
#[case("Binary:1.2.3", "API:1.2.3", false)] // decreasing strictness
#[case("1.2.3", "API:1.2.3", true)] // increasing strictness
#[case("1.2.3", "Binary:1.2.3", true)] // increasing strictness
#[case("API:1.2.3", "1.2.3", false)] // decreasing strictness
#[case("Binary:1.2.3", "1.2.3", false)] // decreasing strictness
#[case(">1.0", "Binary:1.2.3", true)] // increasing strictness; narrowing range
#[case(">2.0", "Binary:1.2.3", false)] // increasing strictness; widening range
// CompatRange special case
#[case("Binary:1.2.3", "=1.2.3", true)] // matching version
#[case("Binary:1.2.3", "==1.2.3", true)] // matching version
#[case("Binary:1.2.3+r.1", "=1.2.3", false)]
#[case("Binary:1.2.4", "=1.2.3", false)]
fn test_contains(#[case] range1: &str, #[case] range2: &str, #[case] expected: bool) {
    let a = parse_version_range(range1).unwrap();
    let b = parse_version_range(range2).unwrap();
    let c = a.contains(&b);
    assert_eq!(c.is_ok(), expected, "{} contains {} == {:?}", a, b, c,);
}

#[rstest]
#[case(">1.0,>2.0", Some(">2.0"))]
#[case("<2.0,>1.0", None)]
#[case("<1.0,>2.0", None)]
#[case(">=1.0,>=2.0", Some(">=2.0"))]
// Merge should happen recursively
#[case(">=1.0,>=3.0,>=2.0", Some(">=3.0"))]
#[case(">=1.0,<=3.0,=2.0", Some("=2.0"))]
// CompatRange must preserve CompatRule
#[case("API:1.2.3,Binary:1.2.3", Some("Binary:1.2.3"))] // increasing strictness
#[case("Binary:1.2.3,API:1.2.3", Some("Binary:1.2.3"))] // decreasing strictness
#[case("1.2.3,API:1.2.3", Some("API:1.2.3"))] // increasing strictness
#[case("1.2.3,Binary:1.2.3", Some("Binary:1.2.3"))] // increasing strictness
#[case("API:1.2.3,1.2.3", Some("API:1.2.3"))] // decreasing strictness
#[case("Binary:1.2.3,1.2.3", Some("Binary:1.2.3"))] // decreasing strictness
// CompatRange special case
#[case("=1.2.3,Binary:1.2.3", Some("=1.2.3"))] // matching version
#[case("==1.2.3,Binary:1.2.3", Some("==1.2.3"))] // matching version
#[case("=1.2.3,Binary:1.2.3+r.1", None)]
#[case("=1.2.3,Binary:1.2.4", None)]
fn test_parse_version_range_simplifies(#[case] range1: &str, #[case] expected: Option<&str>) {
    let a = parse_version_range(range1).unwrap();
    match expected.map(|s| parse_version_range(s).unwrap()) {
        Some(expected_vr) => {
            // Some merge was expected.
            assert_eq!(a, expected_vr);
        }
        None => {
            // A merge was _not_ expected
            assert_eq!(a.to_string(), range1);
        }
    }
}
