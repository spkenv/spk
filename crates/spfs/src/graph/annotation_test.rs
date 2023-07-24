// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spfs_encoding::Digest;

use super::AnnotationValue;
use crate::encoding;

#[rstest]
fn test_annotationvalue_string() {
    let value = String::from("value");
    let string_value = AnnotationValue::String(value);

    assert!(string_value.is_string());
    assert!(!string_value.is_blob());
}

#[rstest]
fn test_annotationvalue_blob() {
    let digest: Digest = encoding::EMPTY_DIGEST.into();

    let blob_value = AnnotationValue::Blob(digest);

    assert!(blob_value.is_blob());
    assert!(!blob_value.is_string());
}
