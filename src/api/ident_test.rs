// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{parse_ident, Ident};
use crate::api::{parse_version, Build};

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
    let ident = Ident::new("package").unwrap();
    let out = serde_yaml::to_string(&ident).unwrap();
    assert_eq!(&out, "---\npackage\n");
}

#[rstest]
#[case(
    "hello/1.0.0/src",
    Ident{name: "hello".to_string(), version: parse_version("1.0.0").unwrap(), build: Some(Build::Source)}
)]
#[case(
    "python/2.7",
    Ident{name: "python".to_string(), version: parse_version("2.7").unwrap(), build: None}
)]
fn test_parse_ident(#[case] input: &str, #[case] expected: Ident) {
    let actual = parse_ident(input).unwrap();
    assert_eq!(actual, expected);
}
