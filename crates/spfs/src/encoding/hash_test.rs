// Copyright (c) 2022 Sony Pictures Imageworks, et al.
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
