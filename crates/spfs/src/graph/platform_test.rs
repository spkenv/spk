// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Platform;
use crate::encoding;
use crate::encoding::prelude::*;

#[rstest]
#[serial_test::serial(config)]
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
    assert_eq!(actual.to_stack(), expected.to_stack());
    assert_eq!(actual.inner_bytes(), expected.inner_bytes());
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
}
