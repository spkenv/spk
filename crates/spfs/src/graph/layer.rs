// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs_proto::LayerArgs;

use super::object::HeaderBuilder;
use super::{Annotation, AnnotationValue, ObjectKind};
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./layer_test.rs"]
mod layer_test;

/// Layers represent a logical collection of software artifacts.
///
/// Layers are considered completely immutable, and are
/// uniquely identifiable by the computed hash of all
/// relevant file and metadata.
pub type Layer = super::object::FlatObject<spfs_proto::Layer<'static>>;

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field(
                "manifest",
                &self
                    .manifest()
                    .map_or(String::from("None"), |d| d.to_string()),
            )
            .field("annotations", &self.annotations())
            .finish()
    }
}

impl Layer {
    /// Build a layer with the default header that points
    /// at the provided manifest digest,
    /// for more configuration use [`Self::builder`]
    #[inline]
    pub fn new(manifest: encoding::Digest) -> Self {
        Self::builder().with_manifest(manifest).build()
    }

    /// Build a layer with the default header that has the provided
    /// annotation data but does not point at any manifest, for more
    /// configuration use [`Self::builder`]
    #[inline]
    pub fn new_with_annotation(key: String, value: AnnotationValue) -> Self {
        Self::builder().with_annotation(key, value).build()
    }

    /// Build a layer with the default header that has the provided
    /// annotation data but does not point at any manifest, for more
    /// configuration use [`Self::builder`]
    #[inline]
    pub fn new_with_annotations(annotations: Vec<KeyAnnotationValuePair>) -> Self {
        Self::builder().with_annotations(annotations).build()
    }

    /// Build a layer with the default header that points at the
    /// provided manifest digest and the provided annotation, for
    /// more configuration use [`Self::builder`]
    #[inline]
    pub fn new_with_manifest_and_annotation(
        manifest: encoding::Digest,
        key: String,
        value: AnnotationValue,
    ) -> Self {
        Self::builder()
            .with_manifest(manifest)
            .with_annotation(key, value)
            .build()
    }

    /// Build a layer with the default header that points at the
    /// provided manifest digest and the provided annotation, for
    /// more configuration use [`Self::builder`]
    #[inline]
    pub fn new_with_manifest_and_annotations(
        manifest: encoding::Digest,
        annotations: Vec<KeyAnnotationValuePair>,
    ) -> Self {
        Self::builder()
            .with_manifest(manifest)
            .with_annotations(annotations)
            .build()
    }

    #[inline]
    pub fn builder() -> LayerBuilder {
        LayerBuilder::default()
    }

    #[inline]
    pub fn manifest(&self) -> Option<&encoding::Digest> {
        self.proto().manifest()
    }

    #[inline]
    pub fn annotations(&self) -> Vec<spfs_proto::Annotation> {
        self.proto()
            .annotations()
            .iter()
            .collect::<Vec<spfs_proto::Annotation>>()
    }

    /// Return the child object of this one in the object DG.
    #[inline]
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        let mut children = Vec::new();
        if let Some(manifest_digest) = self.manifest() {
            children.push(*manifest_digest)
        }
        for entry in self.annotations() {
            let annotation: Annotation = entry.into();
            children.extend(annotation.child_objects());
        }
        children
    }

    pub(super) fn digest_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        // Includes any annotations regardless of the EncodingFormat setting
        let annotations = self.annotations();
        let result = if let Some(manifest_digest) = self.manifest() {
            let manifest_result =
                encoding::write_digest(&mut writer, manifest_digest).map_err(Error::Encoding);
            for entry in annotations {
                let annotation: Annotation = entry.into();
                annotation.legacy_encode(&mut writer)?;
            }
            manifest_result
        } else if !annotations.is_empty() {
            for entry in annotations {
                let annotation: Annotation = entry.into();
                annotation.legacy_encode(&mut writer)?;
            }
            Ok(())
        } else {
            Err(Error::String(
                "Invalid Layer object for legacy encoding, it has no manifest or annotation data"
                    .to_string(),
            ))
        };

        result
    }

    pub(super) fn legacy_encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        // Legacy encoded layers do not support writing annotations
        let result = if let Some(manifest_digest) = self.manifest() {
            encoding::write_digest(writer, manifest_digest).map_err(Error::Encoding)
        } else {
            Err(Error::String(
                "Invalid Layer object for legacy encoding, it has no manifest data".to_string(),
            ))
        };

        result
    }
}

impl std::hash::Hash for Layer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.proto().manifest().hash(state)
    }
}

impl std::cmp::PartialEq for Layer {
    fn eq(&self, other: &Self) -> bool {
        self.proto().manifest() == other.proto().manifest()
    }
}

impl std::cmp::Eq for Layer {}

/// Data type for pairs of keys and annotation values used during
/// construction of a layer's annotations.
pub type KeyAnnotationValuePair = (String, AnnotationValue);

pub struct LayerBuilder {
    header: super::object::HeaderBuilder,
    manifest: Option<encoding::Digest>,
    annotations: Vec<KeyAnnotationValuePair>,
}

impl Default for LayerBuilder {
    fn default() -> Self {
        Self {
            header: super::object::HeaderBuilder::new(ObjectKind::Layer),
            manifest: None,
            annotations: Vec::new(),
        }
    }
}

impl LayerBuilder {
    pub fn with_header<F>(mut self, mut header: F) -> Self
    where
        F: FnMut(HeaderBuilder) -> HeaderBuilder,
    {
        self.header = header(self.header).with_object_kind(ObjectKind::Layer);
        self
    }

    pub fn with_manifest(mut self, manifest: encoding::Digest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    pub fn with_annotation(mut self, key: String, value: AnnotationValue) -> Self {
        self.annotations.push((key, value));
        self
    }

    pub fn with_annotations(mut self, annotations: Vec<KeyAnnotationValuePair>) -> Self {
        self.annotations.extend(annotations);
        self
    }

    pub fn build(&self) -> Layer {
        super::BUILDER.with_borrow_mut(|builder| {
            let ffb_annotations: Vec<_> = self
                .annotations
                .iter()
                .map(|(k, v)| {
                    let key = builder.create_string(k);
                    let value = v.build(builder);
                    spfs_proto::Annotation::create(
                        builder,
                        &spfs_proto::AnnotationArgs {
                            key: Some(key),
                            data_type: v.to_proto(),
                            data: Some(value),
                        },
                    )
                })
                .collect();
            let annotations = Some(builder.create_vector(&ffb_annotations));

            let layer = spfs_proto::Layer::create(
                builder,
                &LayerArgs {
                    manifest: self.manifest.as_ref(),
                    annotations,
                },
            );
            let any = spfs_proto::AnyObject::create(
                builder,
                &spfs_proto::AnyObjectArgs {
                    object_type: spfs_proto::Object::Layer,
                    object: Some(layer.as_union_value()),
                },
            );
            builder.finish_minimal(any);
            let offset = unsafe {
                // Safety: we have just created this buffer
                // so already know the root type with certainty
                flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                    .object_as_layer()
                    .unwrap()
                    ._tab
                    .loc()
            };
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained layer
                // which is what we've done
                Layer::new_with_header(self.header.build(), builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }

    /// Read a data encoded using the legacy format, and
    /// use the data to fill and complete this builder
    pub fn legacy_decode(self, reader: &mut impl std::io::Read) -> Result<Layer> {
        tracing::trace!("layer legacy_decode called...");
        // Legacy layers do not have an annotation field. Trying
        // to read a layer with no manifest and only an annotation
        // here will fail.
        Ok(self.with_manifest(encoding::read_digest(reader)?).build())
    }
}
