// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, convert::TryFrom, str::FromStr};

use itertools::Itertools;
use nom::{combinator::all_consuming, error::ErrorKind};
use proptest::{
    collection::{btree_map, btree_set, hash_set, vec},
    option::weighted,
    prelude::*,
};
use spk_schema_foundation::ident_build::{Build, EmbeddedSource};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::{PkgNameBuf, RepositoryNameBuf};
use spk_schema_foundation::version::{CompatRule, TagSet, Version};
use spk_schema_foundation::version_range::{
    CompatRange, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, SemverRange, VersionFilter, VersionRange,
    WildcardRange,
};

use crate::{parse_ident, Ident, RangeIdent};

macro_rules! arb_version_range_struct {
    ($arb_name:ident, $type_name:ident, $($var:ident in $strategy:expr),+ $(,)?) => {
        prop_compose! {
            fn $arb_name()($($var in $strategy),+) -> $type_name {
                $type_name::new($($var),+)
            }
        }
    }
}

arb_version_range_struct!(arb_compat_range, CompatRange, base in arb_legal_version(), required in weighted(0.66, arb_compat_rule()));
arb_version_range_struct!(arb_double_equals_version, DoubleEqualsVersion, version in arb_legal_version());
arb_version_range_struct!(arb_equals_version, EqualsVersion, version in arb_legal_version());
arb_version_range_struct!(arb_greater_than_range, GreaterThanRange, bound in arb_legal_version());
arb_version_range_struct!(arb_greater_than_or_equal_to_range, GreaterThanOrEqualToRange, bound in arb_legal_version());
arb_version_range_struct!(arb_less_than_range, LessThanRange, bound in arb_legal_version());
arb_version_range_struct!(arb_less_than_or_equal_to_range, LessThanOrEqualToRange, bound in arb_legal_version());
arb_version_range_struct!(arb_semver_range, SemverRange, minimum in arb_legal_version());

prop_compose! {
    // CompatRule::None intentionally not included in this list.
    fn arb_compat_rule()(cr in prop_oneof![Just(CompatRule::API), Just(CompatRule::Binary)]) -> CompatRule {
        cr
    }
}

prop_compose! {
    fn arb_component()(component in prop_oneof![
        Just(Component::All),
        Just(Component::Run),
        Just(Component::Build),
        Just(Component::Source),
        // components that look like reserved names
        "all[a-z]+".prop_map(Component::Named),
        "run[a-z]+".prop_map(Component::Named),
        "build[a-z]+".prop_map(Component::Named),
        "src[a-z]+".prop_map(Component::Named),
        arb_pkg_legal_name().prop_filter("name can't be a reserved name", |name| !(name == "all" || name == "run" || name == "build" || name == "src")).prop_map(|name| Component::Named(name.into_inner())),
    ]) -> Component {
        component
    }
}

prop_compose! {
    fn arb_components()(components in hash_set(arb_component(), 0..10)) -> HashSet<Component> {
        components
    }
}

prop_compose! {
    fn arb_double_not_equals_version()(base in arb_legal_version()) -> DoubleNotEqualsVersion {
        DoubleNotEqualsVersion::from(base)
    }
}

prop_compose! {
    // LowestSpecifiedRange requires there to be at least 2 version elements specified.
    fn arb_lowest_specified_range()(base in arb_legal_version_min_len(LowestSpecifiedRange::REQUIRED_NUMBER_OF_DIGITS)) -> LowestSpecifiedRange {
        // Safety: we generate at least the required minimum of two parts.
        unsafe { LowestSpecifiedRange::try_from(base).unwrap_unchecked() }
    }
}

prop_compose! {
    // These name length limits come from PkgName::MIN_LEN and PkgName::MAX_LEN
    fn arb_pkg_legal_name()(name in "[a-z][a-z-]{1,63}") -> PkgNameBuf {
        // Safety: We only generate names that are valid.
        unsafe { PkgNameBuf::from_string(name) }
    }
}

fn arb_pkg_illegal_name() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        // Names that start with hyphens
        Just("--"),
        // Too short
        Just("a"),
        // Uppercase
        Just("MixedCase"),
        // Symbols other than hyphens
        Just("not+legal"),
        Just("no.dots"),
    ]
}

// May generate an illegal name.
fn arb_pkg_name() -> impl Strategy<Value = (String, bool)> {
    prop_oneof![
        9 => arb_pkg_legal_name().prop_map(|name| (name.to_string(), true)),
        1 => arb_pkg_illegal_name().prop_map(|name| (name.to_string(), false))
    ]
}

prop_compose! {
    fn arb_not_equals_version()(base in arb_legal_version()) -> NotEqualsVersion {
        NotEqualsVersion::from(base)
    }
}

fn arb_opt_legal_version() -> impl Strategy<Value = Option<Version>> {
    weighted(0.9, arb_legal_version())
}

fn arb_opt_illegal_version() -> impl Strategy<Value = Option<Version>> {
    weighted(0.9, arb_illegal_version())
}

// May generate an illegal version.
fn arb_opt_version() -> impl Strategy<Value = (Option<Version>, bool)> {
    prop_oneof![
        9 => arb_opt_legal_version().prop_map(|v| (v, true)),
        1 => arb_opt_illegal_version().prop_map(|v| {
            // If it is None, it is not illegal
            let is_some = v.is_some();
            (v, !is_some)
        })
    ]
}

fn arb_opt_version_filter() -> impl Strategy<Value = Option<VersionFilter>> {
    weighted(0.9, arb_version_filter())
}

prop_compose! {
    fn arb_repo()(name in weighted(0.9, prop_oneof!["local", "origin", arb_pkg_legal_name().prop_map(|name| name.into_inner())])) -> Option<RepositoryNameBuf> {
        // Safety: We only generate legal repository names.
        name.map(|n| unsafe { RepositoryNameBuf::from_string(n) })
    }
}

prop_compose! {
    // This is allowed to generate more than one entry in the `TagSet`
    // because the names are restricted to not be all numeric, which resolves
    // the parsing ambiguity.
    fn arb_unambiguous_tagset()(tags in btree_map(
        prop_oneof![
            "[a-zA-Z][a-zA-Z0-9]*",
            "[a-zA-Z0-9]*[a-zA-Z]",
            "[a-zA-Z0-9]*[a-zA-Z][a-zA-Z0-9]*",
        ],
        any::<u32>(), 0..=10)) -> TagSet {
        TagSet { tags }
    }
}

prop_compose! {
    // XXX: The tagset is limited to a maximum of one entry because of
    // the ambiguous use of commas to delimit both tags and version filters.
    fn arb_ambiguous_tagset()(tags in btree_map("[a-zA-Z0-9]+", any::<u32>(), 0..=1)) -> TagSet {
        TagSet { tags }
    }
}

fn arb_legal_tagset() -> impl Strategy<Value = TagSet> {
    arb_unambiguous_tagset()
}

prop_compose! {
    // XXX: The illegal tagset is limited to a maximum of one entry because of
    // the ambiguous use of commas to delimit both tags and version filters.
    fn arb_illegal_tagset()(tags in btree_map("[0-9]+", any::<u32>(), 1..=1)) -> TagSet {
        TagSet { tags }
    }
}

fn arb_legal_version() -> impl Strategy<Value = Version> {
    arb_legal_version_min_len(1)
}

fn arb_illegal_version() -> impl Strategy<Value = Version> {
    arb_illegal_version_min_len(1)
}

fn arb_legal_version_min_len(min_len: usize) -> impl Strategy<Value = Version> {
    (
        vec(any::<u32>(), min_len..min_len.max(10)).prop_filter(
            // Avoid generating version parts that look like [0, 0].
            // Remove this after #370 is merged.
            //
            // The property tests can generate two different version
            // ranges that have the same string representation:
            // `["0+a.0", "0.0"] == "0+a.0,0.0"
            //
            // Due to tag name vs "0.0" parsing ambiguity, this gets
            // consistently parsed in one way but will fail the test if
            // the prop test generated the other combination of version
            // ranges (and expects it be parsed the other way).
            "No parts of length 2",
            |parts| parts.len() != 2,
        ),
        arb_legal_tagset(),
        arb_legal_tagset(),
    )
        .prop_map(|(parts, pre, post)| Version {
            // We don't expect to generate any values that have `plus_epsilon`
            // enabled.
            parts: parts.into(),
            pre,
            post,
        })
}

fn arb_illegal_version_min_len(min_len: usize) -> impl Strategy<Value = Version> {
    (
        vec(any::<u32>(), min_len..min_len.max(10)),
        any::<bool>(),
        arb_legal_tagset(),
        arb_illegal_tagset(),
    )
        .prop_map(|(parts, use_illegal_for_pre, legal, illegal)| {
            if use_illegal_for_pre {
                Version {
                    // We don't expect to generate any values that have `plus_epsilon`
                    // enabled.
                    parts: parts.into(),
                    pre: illegal,
                    post: legal,
                }
            } else {
                Version {
                    // We don't expect to generate any values that have `plus_epsilon`
                    // enabled.
                    parts: parts.into(),
                    pre: legal,
                    post: illegal,
                }
            }
        })
}

prop_compose! {
    fn arb_version_filter()(rules in btree_set(arb_version_range(), 1..10)) -> VersionFilter {
        VersionFilter::new(rules)
    }
}

fn arb_version_range() -> impl Strategy<Value = VersionRange> {
    let leaf = prop_oneof![
        arb_version_range_compat(),
        arb_version_range_double_equals(),
        arb_version_range_double_not_equals(),
        arb_version_range_equals(),
        // Filter is recursive so it doesn't go in this list.
        arb_version_range_greater_than(),
        arb_version_range_greater_than_or_equal_to(),
        arb_version_range_less_than(),
        arb_version_range_less_than_or_equal_to(),
        arb_version_range_lowest_specified(),
        arb_version_range_not_equals(),
        arb_version_range_semver(),
        arb_version_range_wildcard(),
    ];
    // XXX: Generating a VersionRange::Filter (recursively) is pointless
    // since it becomes flattened when turned into a string, before parsing.
    leaf.prop_recursive(3, 16, 10, |inner| {
        btree_set(inner, 1..10).prop_map(|rules| VersionRange::Filter(VersionFilter::new(rules)))
    })
}

prop_compose! {
    fn arb_version_range_compat()(compat_range in arb_compat_range()) -> VersionRange {
        VersionRange::Compat(compat_range)
    }
}

prop_compose! {
    fn arb_version_range_double_equals()(double_equals_version in arb_double_equals_version()) -> VersionRange {
        VersionRange::DoubleEquals(double_equals_version)
    }
}

prop_compose! {
    fn arb_version_range_double_not_equals()(double_not_equals_version in arb_double_not_equals_version()) -> VersionRange {
        VersionRange::DoubleNotEquals(double_not_equals_version)
    }
}

prop_compose! {
    fn arb_version_range_equals()(equals_version in arb_equals_version()) -> VersionRange {
        VersionRange::Equals(equals_version)
    }
}

prop_compose! {
    fn arb_version_range_filter()(filter_version in arb_version_filter()) -> VersionRange {
       VersionRange::Filter(filter_version)
    }
}

prop_compose! {
    fn arb_version_range_greater_than()(greater_than in arb_greater_than_range()) -> VersionRange {
        VersionRange::GreaterThan(greater_than)
    }
}

prop_compose! {
    fn arb_version_range_greater_than_or_equal_to()(greater_than_or_equal_to in arb_greater_than_or_equal_to_range()) -> VersionRange {
        VersionRange::GreaterThanOrEqualTo(greater_than_or_equal_to)
    }
}

prop_compose! {
    fn arb_version_range_less_than()(less_than in arb_less_than_range()) -> VersionRange {
        VersionRange::LessThan(less_than)
    }
}

prop_compose! {
    fn arb_version_range_less_than_or_equal_to()(less_than_or_equal_to in arb_less_than_or_equal_to_range()) -> VersionRange {
        VersionRange::LessThanOrEqualTo(less_than_or_equal_to)
    }
}

prop_compose! {
    fn arb_version_range_lowest_specified()(lowest_specified_range in arb_lowest_specified_range()) -> VersionRange {
        VersionRange::LowestSpecified(lowest_specified_range)
    }
}

prop_compose! {
    fn arb_version_range_not_equals()(not_equals_version in arb_not_equals_version()) -> VersionRange {
        VersionRange::NotEquals(not_equals_version)
    }
}

prop_compose! {
    fn arb_version_range_semver()(semver_range in arb_semver_range()) -> VersionRange {
        VersionRange::Semver(semver_range)
    }
}

prop_compose! {
    fn arb_version_range_wildcard()(wildcard_range in arb_wildcard_range()) -> VersionRange {
        VersionRange::Wildcard(wildcard_range)
    }
}

prop_compose! {
    fn arb_ident()(
        name in arb_pkg_legal_name(),
        version in arb_legal_version(),
    ) -> Ident {
        // TODO: mutually recursive strategy here Ident -> Build -> Ident.
        // There is a "proptest-recurse" crate but it looks unmaintained.
        Ident { name, version, build: None }
    }
}

fn arb_embedded_build() -> impl Strategy<Value = Build> {
    prop_oneof![
        3 => Just(Build::Embedded(EmbeddedSource::Unknown)),
        7 => arb_ident().prop_map(|ident| Build::Embedded(EmbeddedSource::Ident(ident.to_string()))),
    ]
}

fn arb_build() -> impl Strategy<Value = Option<Build>> {
    weighted(
        0.9,
        prop_oneof![
            1 => Just(Build::Source),
            2 => arb_embedded_build(),
            8 => "[2-7A-Z]{8}"
                .prop_filter("valid BASE32 value", |s| data_encoding::BASE32
                    .decode(s.as_bytes())
                    .is_ok())
                .prop_map(|digest| Build::from_str(digest.as_str()).unwrap())
        ],
    )
}

prop_compose! {
    fn arb_wildcard_range()(
        // Here we generate a non-empty Vec<Option<u32>>,
        // then turn the first element into a None (to represent the '*'),
        // and then shuffle the result. This ensures there is one and only
        // one '*' but its placement is random.
        parts in vec(any::<u32>(), 1..10).prop_map(|v| v.into_iter().enumerate().map(|(index, i)| {
            if index == 0 {
                None
            }
            else {
                Some(i)
            }
        }).collect::<Vec<_>>()).prop_shuffle(),
    ) -> WildcardRange {
        // Safety: we generate the required one and only one optional part.
        unsafe { WildcardRange::new_unchecked(parts) }
    }
}

proptest! {
    #[test]
    fn prop_test_parse_ident(
            (name, name_is_legal) in arb_pkg_name(),
            (version, version_is_legal) in arb_opt_version(),
            build in arb_build()) {
        // If specifying a build, a version must also be specified.
        prop_assume!(build.is_none() || version.is_some());
        let ident = [
            Some(name.clone()),
            version.as_ref().map(|v| {
                v.to_string()
            }),
            build.as_ref().map(|b| b.to_string()),
        ].iter().flatten().join("/");
        let parsed = parse_ident(&ident);
        if name_is_legal && version_is_legal {
            assert!(parsed.is_ok(), "parse '{}' failure:\n{}", ident, parsed.unwrap_err());
            let parsed = parsed.unwrap();
            assert_eq!(parsed.name.as_str(), name);
            assert_eq!(parsed.version, version.unwrap_or_default());
            assert_eq!(parsed.build, build);
        }
        else {
            assert!(parsed.is_err(), "expected '{}' to fail to parse", ident);
        }
    }
}

proptest! {
    #[test]
    fn prop_test_parse_range_ident(
            repo in arb_repo(),
            (name, name_is_legal) in arb_pkg_name(),
            components in arb_components(),
            version in arb_opt_version_filter(),
            build in arb_build()) {
        // If specifying a build, a version must also be specified.
        prop_assume!(build.is_none() || version.is_some());

        let name_and_component_str =
            match components.len() {
                0 => name.clone(),
                1 => format!("{name}:{component}", component = components.iter().next().unwrap()),
                _ => format!("{name}:{{{components}}}", components = components.iter().join(","))
            };

        // Rather than creating a `RangeIdent` here and using `to_string`
        // to generate the test input, the input is generated manually.
        // This avoids any normalization that may reduce the types of
        // inputs that end up getting parsed.
        let ident = [
            repo.as_ref().map(|r| r.as_str().to_owned()),
            Some(name_and_component_str),
            version.as_ref().map(|v| {
                v.to_string()
            }),
            build.as_ref().map(|b| b.to_string()),
        ].iter().flatten().join("/");
        let parsed = RangeIdent::from_str(&ident);
        if name_is_legal {
            assert!(parsed.is_ok(), "parse '{}' failure:\n{}", ident, parsed.unwrap_err());
            let parsed = parsed.unwrap();
            assert_eq!(parsed.repository_name, repo);
            assert_eq!(parsed.name.as_str(), name);
            assert_eq!(parsed.components, components);
            // Must flatten the version_filter we generated to compare with
            // the parsed one, since the parsed one gets flattened too.
            let flattened = version.unwrap_or_default().flatten();
            assert_eq!(parsed.version, flattened, "Parsing: `{}`\n  left: `{}`\n right: `{}`", ident, parsed.version, flattened);
            assert_eq!(parsed.build, build, "{:?} != {:?}", parsed.build, build);
        }
        else {
            assert!(parsed.is_err(), "expected '{}' to fail to parse", ident);
        }
    }
}

/// Invoke the `ident` parser without `VerboseError` for coverage.
#[test]
fn parse_range_ident_with_basic_errors() {
    let empty = HashSet::new();
    let r = crate::parsing::range_ident::<(_, ErrorKind)>(&empty, "pkg-name");
    assert!(r.is_ok(), "{}", r.unwrap_err());
}

/// Invoke the `range_ident` parser without `VerboseError` for coverage.
#[test]
fn parse_ident_with_basic_errors() {
    let r = crate::parsing::ident::<(_, ErrorKind)>("pkg-name");
    assert!(r.is_ok(), "{}", r.unwrap_err());
}

/// Fail if post-tags are specified before pre-tags.
#[test]
fn check_wrong_tag_order_is_a_parse_error() {
    let r = all_consuming(crate::parsing::ident::<(_, ErrorKind)>)("pkg-name/1.0+a.0-b.0");
    assert!(r.is_err(), "expected to fail; got {:?}", r);
}
