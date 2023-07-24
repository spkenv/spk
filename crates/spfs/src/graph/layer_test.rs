// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Layer;
use crate::encoding;
use crate::encoding::prelude::*;
use crate::graph::object::EncodingFormat;
use crate::graph::{AnnotationValue, Object};

#[rstest]
fn test_layer_encoding_manifest_only() {
    let expected = Layer::new(encoding::EMPTY_DIGEST.into());
    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();
    let actual = Object::decode(&mut stream.as_slice())
        .unwrap()
        .into_layer()
        .unwrap();
    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
}

#[rstest]
fn test_layer_encoding_annotation_only() {
    let expected = Layer::new_with_annotation(
        "key".to_string(),
        AnnotationValue::String("value".to_string()),
    );
    tracing::error!("Expected: {:?}", expected);

    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();

    let decoded = Object::decode(&mut stream.as_slice());
    if EncodingFormat::default() == EncodingFormat::Legacy {
        if decoded.is_ok() {
            panic!("This test should fail when EncodingFormat::Legacy is the default")
        }
        // Don't run the rest of the test when EncodingFormat::Legacy is used
        return;
    };

    let actual = decoded.unwrap().into_layer().unwrap();
    println!(" Actual: {:?}", actual);

    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
}

#[rstest]
fn test_layer_encoding_manifest_and_annotations() {
    let expected = Layer::new_with_manifest_and_annotations(
        encoding::EMPTY_DIGEST.into(),
        vec![(
            "key".to_string(),
            AnnotationValue::String("value".to_string()),
        )],
    );
    println!("Expected: {:?}", expected);

    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();

    let actual = Object::decode(&mut stream.as_slice())
        .unwrap()
        .into_layer()
        .unwrap();
    println!(" Actual: {:?}", actual);

    match EncodingFormat::default() {
        EncodingFormat::Legacy => {
            // Legacy encoding does not support annotaion data, so these won't match
            assert_ne!(actual.digest().unwrap(), expected.digest().unwrap())
        }
        EncodingFormat::FlatBuffers => {
            // Under flatbuffers encoding both will contain the
            // annotation data and will match
            assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
        }
    }
}
