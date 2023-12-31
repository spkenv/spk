// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Layer;
use crate::encoding;
use crate::encoding::prelude::*;
use crate::graph::Object;

#[rstest]
fn test_layer_encoding() {
    let expected = Layer::new(encoding::EMPTY_DIGEST.into());
    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();
    let actual = Object::decode(&mut stream.as_slice())
        .unwrap()
        .into_layer()
        .unwrap();
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
}
