// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::{DigestFromEncode, Kind, KindAndEncodeDigest, ObjectKind};
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
pub struct Layer<DigestImpl = DigestFromEncode> {
    pub manifest: encoding::Digest,
    phantom: std::marker::PhantomData<DigestImpl>,
}

impl Layer {
    pub fn new(manifest: encoding::Digest) -> Self {
        Layer {
            manifest,
            phantom: std::marker::PhantomData,
        }
    }

    /// Return the child object of this one in the object DG.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        vec![self.manifest]
    }
}

impl<D> encoding::Encodable for Layer<D> {
    type Error = Error;

    fn encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(writer, &self.manifest).map_err(Error::Encoding)
    }
}

impl encoding::Decodable for Layer {
    fn decode(reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Layer {
            manifest: encoding::read_digest(reader)?,
            phantom: std::marker::PhantomData,
        })
    }
}

impl Kind for Layer {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::Layer
    }
}

impl<D> encoding::Digestible for Layer<D>
where
    Self: Kind,
    D: KindAndEncodeDigest<Error = crate::Error>,
{
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<encoding::Digest, Self::Error> {
        D::digest(self)
    }
}
