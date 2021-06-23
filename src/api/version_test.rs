// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::Ord;
use std::cmp::Ordering;

use rstest::rstest;

use super::{parse_version, TagSet, Version};

#[rstest]
fn test_version_nonzero() {
    assert_eq!(true, Version::default().is_zero());
    assert_eq!(false, Version::new(1, 0, 0).is_zero());
}

#[rstest]
#[case("1.0.0", "1.0.0", false)]
#[case("1", "1.0.0", false)]
#[case("1.0.0", "1", false)]
#[case("6.3", "4.8.5", true)]
#[case("6.3", "6.3+post.0", false)]
#[case("6.3+post.0", "6.3", true)]
#[case("6.3+b.0", "6.3+a.0", true)]
#[case("6.3-pre.0", "6.3", false)]
#[case("6.3", "6.3-pre.0", true)]
#[case("6.3-pre.1", "6.3-pre.0", true)]
#[case("6.3+r.1", "6.3+other.1,r.1", true)]
fn test_is_gt(#[case] base: &str, #[case] test: &str, #[case] expected: bool) {
    let a = parse_version(base).unwrap();
    let b = parse_version(test).unwrap();
    let actual = a > b;
    assert_eq!(actual, expected, "{} should be greater than {}", a, b);
}

#[rstest]
#[case("1.0.0", Version::new(1, 0, 0))]
#[case("0.0.0", Version::new(0, 0, 0))]
#[case("1.2.3.4.5.6", Version{
    major: 1, minor: 2, patch: 3, tail: vec![4, 5, 6], ..Default::default()
})]
#[case("1.0+post.1", Version{
    major: 1, post: TagSet::single("post", 1), ..Default::default()
})]
#[case(
     "1.2.5.7-alpha.4+rev.6",
     Version{
         major: 1, minor: 2, patch: 5, tail: vec![7],
         pre:TagSet::single("alpha", 4), post:TagSet::single("rev", 6)
    },
)]
fn test_parse_version(#[case] string: &str, #[case] expected: Version) {
    let actual = parse_version(string).unwrap();
    assert_eq!(actual, expected)
}

#[rstest]
#[case("1.a.0")]
#[case("my-version")]
#[case("1.0+post.1-pre.2")]
#[case("1.2.5-alpha.a")]
fn test_parse_version_invalid(#[case] string: &str) {
    let result = parse_version(string);
    if let Err(crate::Error::InvalidVersionError(_)) = result {
        // ok
    } else {
        panic!("expected InvalidVersionError, got: {:?}", result)
    }
}

#[rstest]
#[case("1.0.0")]
#[case("0.0.0")]
#[case("1.2.3.4.5.6")]
#[case("1.0+post.1")]
#[case("1.2.5.7-alpha.4+rev.6")]
fn test_parse_version_clone(#[case] string: &str) {
    let v1 = parse_version(string).unwrap();
    let v2 = v1.clone();
    assert_eq!(v1, v2);
}

#[rstest]
#[case(TagSet::single("pre", 1), TagSet::single("pre", 2), Ordering::Less)]
#[case(TagSet::single("pre", 0), TagSet::single("pre", 0), Ordering::Equal)]
#[case(
    TagSet::single("alpha", 0),
    TagSet::double("alpha", 0, "beta", 1),
    Ordering::Less
)]
#[case(TagSet::default(), TagSet::single("alpha", 0), Ordering::Less)]
#[case(TagSet::single("alpha", 0), TagSet::default(), Ordering::Greater)]
#[case(TagSet::single("alpha", 0), TagSet::single("beta", 1), Ordering::Less)]
#[case(TagSet::single("alpha", 0), TagSet::single("alpha", 1), Ordering::Less)]
#[case(
    TagSet::single("alpha", 1),
    TagSet::single("alpha", 1),
    Ordering::Equal
)]
fn test_tag_set_order(#[case] a: TagSet, #[case] b: TagSet, #[case] expected: Ordering) {
    assert_eq!(a.cmp(&b), expected);
}
