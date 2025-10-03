// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Layer;
use crate::encoding::prelude::*;
use crate::graph::object::{DigestStrategy, EncodingFormat};
use crate::graph::{AnnotationValue, Object};
use crate::{Config, encoding};

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

/// A macro to reset the global config after running a block of code,
/// even if that block of code panics.
#[macro_export]
macro_rules! reset_config {
    ($($body:tt)*) => {{
        let reset_config_original = Config::current().unwrap();
        std::panic::catch_unwind(|| {
            $($body)*
        }).unwrap_or_else(|e| {
            (*reset_config_original).clone().make_current().unwrap();

            std::panic::resume_unwind(e);
        });
        (*reset_config_original).clone().make_current().unwrap();
    }};
}

/// A macro to reset the global config after running a block of code,
/// even if that block of code panics.
#[macro_export]
macro_rules! reset_config_async {
    ($($body:tt)*) => {{
        let reset_config_original = Config::current().unwrap();
        let reset_config_handle = tokio::task::spawn(async move {
            $($body)*
        });
        let result = reset_config_handle.await;
        (*reset_config_original).clone().make_current().unwrap();
        match result {
            Ok(_) => {}
            Err(err) if err.is_panic() => {
                let err = err.into_panic();
                std::panic::resume_unwind(err);
            }
            Err(err) => {
                panic!("Task failed to complete: {err}");
            }
        }
    }};
}

/// Sanity test to ensure that the reset_config_async macro catches panics
#[rstest]
#[should_panic]
#[tokio::test]
async fn reset_config_async_catches_panics() {
    reset_config_async! {
        panic!("This is a test panic");

        #[allow(unreachable_code)]
        Ok::<(), ()>(())
    };
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
    reset_config! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let expected = Layer::new_with_annotation("key", AnnotationValue::string("value"));

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
    };
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
    reset_config! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let expected = Layer::new_with_manifest_and_annotations(
            encoding::EMPTY_DIGEST.into(),
            vec![("key", AnnotationValue::string("value"))],
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
}
