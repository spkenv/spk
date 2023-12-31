// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::graph::object::DigestStrategy;
use crate::graph::Platform;
use crate::prelude::*;

#[rstest]
fn test_digest_with_salting() {
    // the digest based on legacy encoding for a platform could easily
    // collide with eight null bytes.
    let legacy_platform = Platform::builder()
        .with_header(|h| h.with_digest_strategy(DigestStrategy::LegacyEncode))
        .build()
        .digest()
        .unwrap();
    let nulls_digest = [0, 0, 0, 0, 0, 0, 0, 0].as_slice().digest().unwrap();
    assert_eq!(legacy_platform, nulls_digest);

    // the newer digest method adds the kind and salt to make
    // such cases less likely
    let salted_platform = Platform::builder()
        .with_header(|h| h.with_digest_strategy(DigestStrategy::EncodeWithKind))
        .build()
        .digest()
        .unwrap();
    assert_ne!(salted_platform, nulls_digest);
}
