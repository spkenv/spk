// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::{Kind, ObjectKind};
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./layer_test.rs"]
mod layer_test;

/// Layers represent a logical collection of software artifacts.
///
/// Layers are considered completely immutable, and are
/// uniquely identifiable by the computed hash of all
/// relevant file and metadata.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Layer {
    pub manifest: encoding::Digest,
}

impl Layer {
    pub fn new(manifest: encoding::Digest) -> Self {
        Layer { manifest }
    }

    /// Return the child object of this one in the object DG.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        vec![self.manifest]
    }
}

impl encoding::Encodable for Layer {
    type Error = Error;

    fn encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(writer, &self.manifest).map_err(Error::Encoding)
    }
}

impl encoding::Decodable for Layer {
    fn decode(reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Layer {
            manifest: encoding::read_digest(reader)?,
        })
    }
}

impl Kind for Layer {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::Layer
    }
}
