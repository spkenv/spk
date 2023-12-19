// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Platform;
use crate::encoding;
use crate::encoding::prelude::*;

#[rstest]
fn test_platform_encoding() {
    let layers: Vec<encoding::Digest> =
        vec![encoding::EMPTY_DIGEST.into(), encoding::NULL_DIGEST.into()];
    let expected = Platform::from_iter(layers);

    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();
    let actual = crate::graph::Object::decode(&mut stream.as_slice())
        .unwrap()
        .into_platform()
        .unwrap();
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}
