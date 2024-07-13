// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Layer;
use crate::encoding::prelude::*;
use crate::graph::object::{DigestStrategy, EncodingFormat};
use crate::graph::{AnnotationValue, Object};
use crate::{encoding, Config};

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

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[serial_test::serial(config)]
fn test_layer_encoding_annotation_only(
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    let mut config = Config::default();
    config.storage.encoding_format = write_encoding_format;
    config.storage.digest_strategy = write_digest_strategy;
    config.make_current().unwrap();

    let expected = Layer::new_with_annotation(
        "key".to_string(),
        AnnotationValue::String("value".to_string()),
    );

    let mut stream = Vec::new();
    match expected.encode(&mut stream) {
        Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
            panic!("Encode should fail if encoding format is legacy")
        }
        Ok(_) => {}
        Err(_) if write_encoding_format == EncodingFormat::Legacy => {
            // This error is expected
            return;
        }
        Err(e) => {
            panic!("Error encoding layer: {e}")
        }
    };

    let decoded = Object::decode(&mut stream.as_slice());

    let actual = decoded.unwrap().into_layer().unwrap();
    println!(" Actual: {:?}", actual);

    assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[serial_test::serial(config)]
fn test_layer_encoding_manifest_and_annotations(
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    let mut config = Config::default();
    config.storage.encoding_format = write_encoding_format;
    config.storage.digest_strategy = write_digest_strategy;
    config.make_current().unwrap();

    let expected = Layer::new_with_manifest_and_annotations(
        encoding::EMPTY_DIGEST.into(),
        vec![(
            "key".to_string(),
            AnnotationValue::String("value".to_string()),
        )],
    );
    println!("Expected: {:?}", expected);

    let mut stream = Vec::new();
    match expected.encode(&mut stream) {
        Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
            panic!("Encode should fail if encoding format is legacy")
        }
        Ok(_) => {}
        Err(_) if write_encoding_format == EncodingFormat::Legacy => {
            // This error is expected
            return;
        }
        Err(e) => {
            panic!("Error encoding layer: {e}")
        }
    };

    let actual = Object::decode(&mut stream.as_slice())
        .unwrap()
        .into_layer()
        .unwrap();
    println!(" Actual: {:?}", actual);

    match write_encoding_format {
        EncodingFormat::Legacy => {
            unreachable!();
        }
        EncodingFormat::FlatBuffers => {
            // Under flatbuffers encoding both will contain the
            // annotation data and will match
            assert_eq!(actual.digest().unwrap(), expected.digest().unwrap())
        }
    }
}
