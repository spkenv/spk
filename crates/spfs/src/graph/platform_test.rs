// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Platform;
use crate::encoding;
use crate::encoding::{Decodable, Encodable};
use crate::graph::DigestFromEncode;

#[rstest]
fn test_platform_encoding() {
    let layers: Vec<encoding::Digest> =
        vec![encoding::EMPTY_DIGEST.into(), encoding::NULL_DIGEST.into()];
    let expected = Platform::<DigestFromEncode>::from_digestible(layers).unwrap();

    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();
    let actual = Platform::decode(&mut stream.as_slice()).unwrap();
    assert_eq!(actual, expected);
}
