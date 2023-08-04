// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::PromotionPatterns;

#[rstest]
#[case("gcc", &["a", "b", "gcc"], &["gcc", "a", "b"])]
#[case::pattern_order_matters_1("gcc,python", &["a", "python", "b", "gcc"], &["gcc", "python", "a", "b"])]
#[case::pattern_order_matters_2("python,gcc", &["a", "python", "b", "gcc"], &["python", "gcc", "a", "b"])]
#[case::pattern_glob("*platform*,python,gcc", &["a", "python", "b", "gcc", "spi-platform"], &["spi-platform", "python", "gcc", "a", "b"])]
fn test_promote_names(#[case] patterns: &str, #[case] input: &[&str], #[case] expected: &[&str]) {
    let patterns = PromotionPatterns::new(patterns);
    let mut subject = input.to_owned();
    patterns.promote_names(subject.as_mut_slice(), |n| n);
    assert_eq!(subject, expected)
}
