// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, str::FromStr};

use itertools::Itertools;
use nom::{combinator::rest, error::ErrorKind};
use proptest::{
    collection::{btree_map, btree_set, hash_set, vec},
    option::weighted,
    prelude::*,
};

use crate::api::{
    parse_ident, Build, CompatRange, CompatRule, Component, DoubleEqualsVersion,
    DoubleNotEqualsVersion, EqualsVersion, GreaterThanOrEqualToRange, GreaterThanRange,
    LessThanOrEqualToRange, LessThanRange, LowestSpecifiedRange, NotEqualsVersion, PkgNameBuf,
    RangeIdent, RepositoryName, SemverRange, TagSet, Version, VersionFilter, VersionRange,
    WildcardRange,
};

macro_rules! arb_version_range_struct {
    ($arb_name:ident, $type_name:ident, $($var:ident in $strategy:expr),+ $(,)?) => {
        prop_compose! {
            fn $arb_name()($($var in $strategy),+) -> $type_name {
                $type_name {
                    $($var),+
                }
            }
        }
    }
}

arb_version_range_struct!(arb_compat_range, CompatRange, base in arb_version(), required in weighted(0.66, arb_compat_rule()));
arb_version_range_struct!(arb_double_equals_version, DoubleEqualsVersion, version in arb_version());
arb_version_range_struct!(arb_equals_version, EqualsVersion, version in arb_version());
arb_version_range_struct!(arb_greater_than_range, GreaterThanRange, bound in arb_version());
arb_version_range_struct!(arb_greater_than_or_equal_to_range, GreaterThanOrEqualToRange, bound in arb_version());
arb_version_range_struct!(arb_less_than_range, LessThanRange, bound in arb_version());
arb_version_range_struct!(arb_less_than_or_equal_to_range, LessThanOrEqualToRange, bound in arb_version());
arb_version_range_struct!(arb_semver_range, SemverRange, minimum in arb_version());

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
        arb_pkg_name().prop_filter("name can't be a reserved name", |name| !(name == "all" || name == "run" || name == "build" || name == "src")).prop_map(|name| Component::Named(name.into_inner())),
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
    fn arb_double_not_equals_version()(base in arb_version()) -> DoubleNotEqualsVersion {
        DoubleNotEqualsVersion {
            specified: base.parts.len(),
            base,
        }
    }
}

prop_compose! {
    // LowestSpecifiedRange requires there to be at least 2 version elements specified.
    fn arb_lowest_specified_range()(base in arb_version_min_len(2)) -> LowestSpecifiedRange {
        LowestSpecifiedRange {
            specified: base.parts.len(),
            base,
        }
    }
}

prop_compose! {
    // These name length limits come from PkgName::MIN_LEN and PkgName::MAX_LEN
    fn arb_pkg_name()(name in "[a-z-]{2,64}") -> PkgNameBuf {
        // Safety: We only generate names that are valid.
        unsafe { PkgNameBuf::from_string(name) }
    }
}

prop_compose! {
    fn arb_not_equals_version()(base in arb_version()) -> NotEqualsVersion {
        NotEqualsVersion {
            specified: base.parts.len(),
            base,
        }
    }
}

fn arb_opt_version() -> impl Strategy<Value = Option<Version>> {
    weighted(0.9, arb_version())
}

fn arb_opt_version_filter() -> impl Strategy<Value = Option<VersionFilter>> {
    weighted(0.9, arb_version_filter())
}

prop_compose! {
    fn arb_repo()(name in weighted(0.9, prop_oneof!["local", "origin", arb_pkg_name().prop_map(|name| name.into_inner())])) -> Option<RepositoryName> {
        name.map(RepositoryName)
    }
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
            // We don't expect to generate any values that have `plus_epsilon`
            // enabled.
            parts: parts.into(),
            pre,
            post,
        })
}

prop_compose! {
    fn arb_version_filter()(rules in btree_set(arb_version_range(), 1..10)) -> VersionFilter {
        VersionFilter { rules }
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
        btree_set(inner, 1..10).prop_map(|rules| VersionRange::Filter(VersionFilter { rules }))
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

fn arb_build() -> impl Strategy<Value = Option<Build>> {
    weighted(
        0.9,
        prop_oneof![
            Just(Build::Source),
            Just(Build::Embedded),
            "[2-7A-Z]{8}"
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
        WildcardRange {
            specified: parts.len(),
            parts,
        }
    }
}

proptest! {
    #[test]
    fn prop_test_parse_ident(
            repo in arb_repo(),
            name in arb_pkg_name(),
            version in arb_opt_version(),
            use_alternate_version in any::<bool>(),
            build in arb_build()) {
        // If specifying a build, a version must also be specified.
        prop_assume!(build.is_none() || version.is_some());
        let ident = [
            repo.as_ref().map(|r| r.0.to_owned()),
            Some(name.clone().into_inner()),
            version.as_ref().map(|v| {
                if use_alternate_version {
                    format!("{:#}", v)
                }
                else {
                    v.to_string()
                }
            }),
            build.as_ref().map(|b| b.to_string()),
        ].iter().flatten().join("/");
        let parsed = parse_ident(&ident);
        assert!(parsed.is_ok(), "parse '{}' failure:\n{}", ident, parsed.unwrap_err());
        let parsed = parsed.unwrap();
        // XXX: This doesn't handle the ambiguous corner cases as checked
        // by `test_parse_ident`; such inputs are very unlikely to be
        // generated randomly here.
        assert_eq!(parsed.repository_name, repo);
        assert_eq!(parsed.name, name);
        assert_eq!(parsed.version, version.unwrap_or_default());
        assert_eq!(parsed.build, build);
    }
}

proptest! {
    #[test]
    fn prop_test_parse_range_ident(
            repo in arb_repo(),
            name in arb_pkg_name(),
            components in arb_components(),
            version in arb_opt_version_filter(),
            use_alternate_version in any::<bool>(),
            build in arb_build()) {
        // If specifying a build, a version must also be specified.
        prop_assume!(build.is_none() || version.is_some());

        let name_and_component_str =
            match components.len() {
                0 => name.clone().into_inner(),
                1 => format!("{name}:{component}", component = components.iter().next().unwrap()),
                _ => format!("{name}:{{{components}}}", components = components.iter().join(","))
            };

        // Rather than creating a `RangeIdent` here and using `to_string`
        // to generate the test input, the input is generated manually.
        // This avoids any normalization that may reduce the types of
        // inputs that end up getting parsed.
        let ident = [
            repo.as_ref().map(|r| r.0.to_owned()),
            Some(name_and_component_str),
            version.as_ref().map(|v| {
                if use_alternate_version {
                    format!("{:#}", v)
                }
                else {
                    v.to_string()
                }
            }),
            build.as_ref().map(|b| b.to_string()),
        ].iter().flatten().join("/");
        let parsed = RangeIdent::from_str(&ident);
        assert!(parsed.is_ok(), "parse '{}' failure:\n{}", ident, parsed.unwrap_err());
        let parsed = parsed.unwrap();
        assert_eq!(parsed.repository_name, repo);
        assert_eq!(parsed.name, name);
        assert_eq!(parsed.components, components);
        // Must flatten the version_filter we generated to compare with
        // the parsed one, since the parsed one gets flattened too.
        let flattened = version.unwrap_or_default().flatten();
        assert_eq!(parsed.version, flattened, "Parsing: `{}`\n  left: `{}`\n right: `{}`", ident, parsed.version, flattened);
        assert_eq!(parsed.build, build);
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
    let empty = HashSet::new();
    let r = crate::parsing::ident::<(_, ErrorKind)>(&empty, "pkg-name");
    assert!(r.is_ok(), "{}", r.unwrap_err());
}

#[test]
fn test_parse_until() {
    let (input, result) =
        crate::parsing::parse_until::<_, _, (_, ErrorKind)>("p", rest)("my input").unwrap();
    assert_eq!(input, "put");
    assert_eq!(result, "my in");

    // Empty input is not an error.
    let (input, result) =
        crate::parsing::parse_until::<_, _, (_, ErrorKind)>("p", rest)("").unwrap();
    assert_eq!(input, "");
    assert_eq!(result, "");
}
