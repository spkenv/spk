// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;

use rstest::rstest;

use super::{AnyIdent, parse_ident};

#[rstest]
#[case("package")]
#[case("package/1.1.0")]
#[case("package/2.0.0.1")]
#[case("package/2.0.0/embedded")]
#[case("package/2.0.0/src")]
#[case("package/2.0.0/BGSHW3CN")]
fn test_ident_to_str(#[case] input: &str) {
    let ident = parse_ident(input).unwrap();
    let out = ident.to_string();
    assert_eq!(out, input);
}

#[rstest]
fn test_ident_to_string() {
    let ident = AnyIdent::from_str("package").unwrap();
    let out = ident.to_string();
    assert_eq!(&out, "package");
}
