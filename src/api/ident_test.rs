// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use itertools::Itertools;
use proptest::{
    collection::{btree_map, vec},
    option::weighted,
    prelude::*,
};
use rstest::rstest;

use super::{parse_ident, Ident, RepositoryName};
use crate::api::{parse_version, Build, PkgNameBuf, TagSet, Version};

#[rstest]
#[case("package")]
#[case("package/1.1.0")]
#[case("package/2.0.0.1")]
fn test_ident_to_str(#[case] input: &str) {
    let ident = parse_ident(input).unwrap();
    let out = ident.to_string();
    assert_eq!(out, input);
}

#[rstest]
fn test_ident_to_yaml() {
    let ident = Ident::from_str("package").unwrap();
    let out = serde_yaml::to_string(&ident).unwrap();
    assert_eq!(&out, "---\npackage\n");
}

#[rstest]
#[case(
    "local/hello/1.0.0/src",
    Ident{repository_name: Some(RepositoryName("local".to_string())), name: "hello".parse().unwrap(), version: parse_version("1.0.0").unwrap(), build: Some(Build::Source)}
)]
#[case(
    "local/hello",
    Ident{repository_name: Some(RepositoryName("local".to_string())), name: "hello".parse().unwrap(), version: Version::default(), build: None}
)]
#[case(
    "hello/1.0.0/src",
    Ident{
        repository_name: None,
        name: "hello".parse().unwrap(),
        version: parse_version("1.0.0").unwrap(),
        build: Some(Build::Source)
    }
)]
#[case(
    "python/2.7",
    Ident{
        repository_name: None,
        name: "python".parse().unwrap(),
        version: parse_version("2.7").unwrap(),
        build: None
    }
)]
#[case(
    "python/2.7-r.1",
    Ident{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7-r.1").unwrap(), build: None}
)]
#[case(
    "python/2.7+r.1",
    Ident{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7+r.1").unwrap(), build: None}
)]
#[case(
    "python/2.7-r.1+r.1",
    Ident{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7-r.1+r.1").unwrap(), build: None}
)]
// pathological cases: package named "local"
#[case(
    "local/1.0.0/src",
    Ident{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap(), build: Some(Build::Source)}
)]
#[case(
    "local/1.0.0/DEADBEEF",
    Ident{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap(), build: Some(Build::from_str("DEADBEEF").unwrap())}
)]
#[case(
    "local/1.0.0",
    Ident{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap(), build: None}
)]
// pathological cases: names that could be version numbers
#[case(
    "111/222/333",
    Ident{repository_name: Some(RepositoryName("111".to_string())), name: "222".parse().unwrap(), version: parse_version("333").unwrap(), build: None}
)]
#[case(
    "222/333",
    Ident{repository_name: None, name: "222".parse().unwrap(), version: parse_version("333").unwrap(), build: None}
)]
#[case(
    "222/333/44444444",
    Ident{repository_name: None, name: "222".parse().unwrap(), version: parse_version("333").unwrap(), build: Some(Build::from_str("44444444").unwrap())}
)]
#[case(
    "local/222",
    Ident{repository_name: Some(RepositoryName("local".to_string())), name: "222".parse().unwrap(), version: Version::default(), build: None}
)]
#[case(
    // like the "222/333" case but with a package name that
    // starts with a known repository name.
    "localx/333",
    Ident{repository_name: None, name: "localx".parse().unwrap(), version: parse_version("333").unwrap(), build: None}
)]
fn test_parse_ident(#[case] input: &str, #[case] expected: Ident) {
    let actual = parse_ident(input).unwrap();
    assert_eq!(actual, expected);
}

prop_compose! {
    // These name length limits come from PkgName::MIN_LEN and PkgName::MAX_LEN
    fn arb_pkg_name()(name in "[a-z-]{2,64}") -> PkgNameBuf {
        // Safety: We only generate names that are valid.
        unsafe { PkgNameBuf::from_string(name) }
    }
}

prop_compose! {
    fn arb_repo()(name in weighted(0.9, prop_oneof!["local", "origin", arb_pkg_name().prop_map(|name| name.into_inner())])) -> Option<RepositoryName> {
        name.map(RepositoryName)
    }
}

prop_compose! {
    fn arb_tagset()(tags in btree_map("[a-zA-Z0-9]+", any::<u32>(), 0..10)) -> TagSet {
        TagSet { tags }
    }
}

fn arb_version() -> impl Strategy<Value = Option<Version>> {
    weighted(
        0.9,
        (vec(any::<u32>(), 0..10), arb_tagset(), arb_tagset()).prop_map(|(parts, pre, post)| {
            Version {
                // We don't expect to generate any values that have
                // `plus_epsilon` enabled.
                parts: parts.into(),
                pre,
                post,
            }
        }),
    )
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

proptest! {
    #[test]
    fn prop_test_parse_ident(
            repo in arb_repo(),
            name in arb_pkg_name(),
            version in arb_version(),
            build in arb_build()) {
        // If specifying a build, a version must also be specified.
        prop_assume!(build.is_none() || version.is_some());
        let ident = [
            repo.as_ref().map(|r| r.0.to_owned()),
            Some(name.clone().into_inner()),
            version.as_ref().map(|v| v.to_string()),
            build.as_ref().map(|b| b.to_string()),
        ].iter().flatten().join("/");
        let parsed = parse_ident(&ident).unwrap();
        // XXX: This doesn't handle the ambiguous corner cases as checked
        // by `test_parse_ident`; such inputs are very unlikely to be
        // generated randomly here.
        assert_eq!(parsed.repository_name, repo);
        assert_eq!(parsed.name, name);
        assert_eq!(parsed.version, version.unwrap_or_default());
        assert_eq!(parsed.build, build);
    }
}
