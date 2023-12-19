// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs_proto::LayerArgs;

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
            .field("manifest", self.manifest())
            .finish()
    }
}

impl Layer {
    pub fn new(manifest: &encoding::Digest) -> Self {
        super::BUILDER.with_borrow_mut(|builder| {
            let layer = spfs_proto::Layer::create(
                builder,
                &LayerArgs {
                    manifest: Some(manifest),
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
                Self::with_default_header(builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }

    pub fn manifest(&self) -> &encoding::Digest {
        self.proto().manifest()
    }

    /// Return the child object of this one in the object DG.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        vec![*self.manifest()]
    }

    pub(super) fn legacy_encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(writer, self.manifest()).map_err(Error::Encoding)
    }

    pub(super) fn legacy_decode(reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Layer::new(&encoding::read_digest(reader)?))
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
