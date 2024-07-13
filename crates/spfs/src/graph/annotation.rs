// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Display;

use crate::{encoding, Result};

#[cfg(test)]
#[path = "./annotation_test.rs"]
mod annotation_test;

/// Default size limit for string valus stored directly in an
/// annotation object. Values larger than this are stored in a blob
/// that is referenced from the annotation object.
pub const DEFAULT_SPFS_ANNOTATION_LAYER_MAX_STRING_VALUE_SIZE: usize = 16 * 1024;

/// Legacy encoding values for distinguishing the kind of
/// AnnotationValue being encoded.
#[repr(u8)]
enum AnnotationValueKind {
    String = 1,
    Blob = 2,
}

/// Wrapper for the ways annotation values are stored
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AnnotationValue {
    /// In the Annotation object as a string
    String(String),
    /// In a separate blob payload pointed at by the digest
    Blob(encoding::Digest),
}

impl Default for AnnotationValue {
    fn default() -> Self {
        AnnotationValue::String(Default::default())
    }
}

impl AnnotationValue {
    pub fn build(
        &self,
        builder: &mut flatbuffers::FlatBufferBuilder<'_>,
    ) -> flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset> {
        match self {
            AnnotationValue::String(data_string) => {
                let string_data = builder.create_string(data_string.as_ref());
                spfs_proto::AnnotationString::create(
                    builder,
                    &spfs_proto::AnnotationStringArgs {
                        data: Some(string_data),
                    },
                )
                .as_union_value()
            }
            AnnotationValue::Blob(data_digest) => spfs_proto::AnnotationDigest::create(
                builder,
                &spfs_proto::AnnotationDigestArgs {
                    digest: Some(data_digest),
                },
            )
            .as_union_value(),
        }
    }

    /// The underlying spfs_proto enum entry for this kind of value.
    pub fn to_proto(&self) -> spfs_proto::AnnotationValue {
        match self {
            AnnotationValue::String(_) => spfs_proto::AnnotationValue::AnnotationString,
            AnnotationValue::Blob(_) => spfs_proto::AnnotationValue::AnnotationDigest,
        }
    }

    /// True if this value is stored directly as a string
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    /// True if this value is stored in-directly in blob object
    pub fn is_blob(&self) -> bool {
        matches!(self, Self::Blob(_))
    }

    /// Note: This is used for legacy and new style digest calculations
    pub fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        match self {
            AnnotationValue::String(v) => {
                encoding::write_uint8(&mut writer, AnnotationValueKind::String as u8)?;
                Ok(encoding::write_string(writer, v.as_str())?)
            }
            AnnotationValue::Blob(v) => {
                encoding::write_uint8(&mut writer, AnnotationValueKind::Blob as u8)?;
                Ok(encoding::write_digest(writer, v)?)
            }
        }
    }
}

impl Display for AnnotationValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnotationValue::String(v) => {
                write!(f, "{v}")
            }
            AnnotationValue::Blob(v) => {
                write!(f, "{v}")
            }
        }
    }
}

/// Annotation represents a key-value pair of data from an external
/// program injected into a spfs runtime layer for later use by another
/// external program.
#[derive(Copy, Clone)]
pub struct Annotation<'buf>(pub(super) spfs_proto::Annotation<'buf>);

impl<'buf> std::fmt::Debug for Annotation<'buf> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Annotation")
            .field("key", &self.key())
            .field("value", &self.value())
            .finish()
    }
}

impl<'buf> From<spfs_proto::Annotation<'buf>> for Annotation<'buf> {
    fn from(value: spfs_proto::Annotation<'buf>) -> Self {
        Self(value)
    }
}

impl<'buf> Annotation<'buf> {
    #[inline]
    pub fn key(&self) -> &'buf str {
        self.0.key()
    }

    pub fn value(&self) -> AnnotationValue {
        if let Some(data) = self.0.data_as_annotation_string() {
            match data.data() {
                Some(s) => AnnotationValue::String(s.into()),
                None => {
                    panic!("This should not happen because the data type was AnnotationValueString")
                }
            }
        } else if let Some(data) = self.0.data_as_annotation_digest() {
            match data.digest() {
                Some(d) => AnnotationValue::Blob(*d),
                None => {
                    panic!("This should not happen because the data type was AnnotationValueDigest")
                }
            }
        } else {
            panic!("This should not happen because the data type was AnnotationValueDigest")
        }
    }

    /// Return the child objects of this object, if any.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        let mut result = Vec::new();
        if let Some(data) = self.0.data_as_annotation_digest() {
            if let Some(d) = data.digest() {
                result.push(*d)
            }
        }
        result
    }

    /// Return the size of this annotation
    pub fn size(&self) -> u64 {
        match &self.value() {
            AnnotationValue::String(v) => v.len() as u64,
            AnnotationValue::Blob(d) => d.len() as u64,
        }
    }

    // Note: This is used for legacy and new style digest calculations
    pub fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_string(&mut writer, self.key())?;
        self.value().legacy_encode(&mut writer)?;
        Ok(())
    }
}

impl<'buf> std::fmt::Display for Annotation<'buf> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}: {:?}", self.key(), self.value()))
    }
}
