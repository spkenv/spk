// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spfs_encoding::Digest;

use super::AnnotationValue;
use crate::encoding;

#[rstest]
fn test_annotation_value_string() {
    let string_value = AnnotationValue::string("value");

    assert!(string_value.is_string());
    assert!(!string_value.is_blob());
}

#[rstest]
fn test_annotation_value_blob() {
    let digest: Digest = encoding::EMPTY_DIGEST.into();

    let blob_value = AnnotationValue::blob(digest);

    assert!(blob_value.is_blob());
    assert!(!blob_value.is_string());
}
