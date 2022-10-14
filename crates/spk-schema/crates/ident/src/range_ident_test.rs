// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::BTreeSet;

use rstest::rstest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::version_range::RestrictMode;

use super::parse_ident_range;

#[rstest]
#[case("python/3.1.0", &[])]
#[case("python:lib/3.1.0", &["lib"])]
#[case("python:{lib}/3.1.0", &["lib"])]
#[case("python:{lib,bin}/3.1.0", &["lib", "bin"])]
#[case("python:{lib,bin,dev}/3.1.0", &["lib", "bin", "dev"])]
#[should_panic]
#[case("python.Invalid/3.1.0", &[""])]
#[should_panic]
#[case("python.lib,bin/3.1.0", &[""])]
fn test_parse_ident_range_components(#[case] source: &str, #[case] expected: &[&str]) {
    let actual = parse_ident_range(source).unwrap();
    let expected: BTreeSet<_> = expected
        .iter()
        .map(Component::parse)
        .map(Result::unwrap)
        .collect();
    assert_eq!(actual.components, expected);
}

#[rstest]
fn test_range_ident_restrict_components() {
    let mut first = parse_ident_range("python:lib").unwrap();
    let second = parse_ident_range("python:bin").unwrap();
    let expected = parse_ident_range("python:{bin,lib}").unwrap();
    first
        .restrict(&second, RestrictMode::RequireIntersectingRanges)
        .unwrap();
    assert_eq!(first.components, expected.components);
}
