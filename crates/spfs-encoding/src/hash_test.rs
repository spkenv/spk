// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

#[rstest]
fn test_partial_digest_empty() {
    assert!(
        super::PartialDigest::parse("").is_err(),
        "empty string is not a valid partial digest"
    )
}

#[rstest]
#[case("AA")]
#[case("BB")]
#[case("CCAA")]
#[should_panic]
#[case("CCA")] // must be multiple of two
fn test_partial_digest_parse(#[case] src: &str) {
    let partial = super::PartialDigest::parse(src).expect("should be valid partial digest");
    let other = partial.to_string().parse().expect("re-parse same partial");
    assert_eq!(partial, other, "should survive a round-trip encoding");
}
