// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::convert::TryInto;
use std::str::FromStr;

use rstest::rstest;
use spk_foundation::ident_build::Build;
use spk_foundation::version::{parse_version, Version};
use spk_foundation::version_range::{CompatRange, VersionFilter, VersionRange};

use super::{parse_ident, Ident};
use crate::RangeIdent;

trait IntoCompatRange {
    fn into_compat_range(self) -> VersionFilter;
}

impl IntoCompatRange for Version {
    fn into_compat_range(self) -> VersionFilter {
        // Typically a `Version` is converted into a `DoubleEquals` but this
        // converts to a `Compat` instead.
        VersionFilter::single(VersionRange::Compat(CompatRange::new(self, None)))
    }
}

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
    RangeIdent{repository_name: Some("local".try_into().unwrap()), name: "hello".parse().unwrap(), version: parse_version("1.0.0").unwrap().into_compat_range(), components: HashSet::default(), build: Some(Build::Source)}
)]
#[case(
    "local/hello",
    RangeIdent{repository_name: Some("local".try_into().unwrap()), name: "hello".parse().unwrap(), version: VersionFilter::default(), components: HashSet::default(), build: None}
)]
#[case(
    "hello/1.0.0/src",
    RangeIdent{
        repository_name: None,
        name: "hello".parse().unwrap(),
        version: parse_version("1.0.0").unwrap().into_compat_range(),
        components: HashSet::default(),
        build: Some(Build::Source)
    }
)]
#[case(
    "python/2.7",
    RangeIdent{
        repository_name: None,
        name: "python".parse().unwrap(),
        version: parse_version("2.7").unwrap().into_compat_range(),
        components: HashSet::default(),
        build: None
    }
)]
#[case(
    "python/2.7-r.1",
    RangeIdent{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7-r.1").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
#[case(
    "python/2.7+r.1",
    RangeIdent{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7+r.1").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
#[case(
    "python/2.7-r.1+r.1",
    RangeIdent{repository_name: None, name: "python".parse().unwrap(), version: parse_version("2.7-r.1+r.1").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
// pathological cases: package named "local"
#[case(
    "local/1.0.0/src",
    RangeIdent{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap().into_compat_range(), components: HashSet::default(), build: Some(Build::Source)}
)]
#[case(
    "local/1.0.0/DEADBEEF",
    RangeIdent{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap().into_compat_range(), components: HashSet::default(), build: Some(Build::from_str("DEADBEEF").unwrap())}
)]
#[case(
    "local/1.0.0",
    RangeIdent{repository_name: None, name: "local".parse().unwrap(), version: parse_version("1.0.0").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
// pathological cases: names that could be version numbers
#[case(
    "111/222/333",
    RangeIdent{repository_name: Some("111".try_into().unwrap()), name: "222".parse().unwrap(), version: parse_version("333").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
#[case(
    "222/333",
    RangeIdent{repository_name: None, name: "222".parse().unwrap(), version: parse_version("333").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
#[case(
    "222/333/44444444",
    RangeIdent{repository_name: None, name: "222".parse().unwrap(), version: parse_version("333").unwrap().into_compat_range(), components: HashSet::default(), build: Some(Build::from_str("44444444").unwrap())}
)]
#[case(
    "local/222",
    RangeIdent{repository_name: Some("local".try_into().unwrap()), name: "222".parse().unwrap(), version: VersionFilter::default(), components: HashSet::default(), build: None}
)]
#[case(
    // like the "222/333" case but with a package name that
    // starts with a known repository name.
    "localx/333",
    RangeIdent{repository_name: None, name: "localx".parse().unwrap(), version: parse_version("333").unwrap().into_compat_range(), components: HashSet::default(), build: None}
)]
fn test_parse_range_ident(#[case] input: &str, #[case] expected: RangeIdent) {
    let actual = RangeIdent::from_str(input).unwrap();
    assert_eq!(actual, expected);
}
