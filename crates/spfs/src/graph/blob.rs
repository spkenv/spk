// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use encoding::Digestible;

use super::object::Kind;
use super::ObjectKind;
use crate::{encoding, Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SameAsPayload {}

trait BlobDigest {
    type Error;

    fn digest<T>(blob: &Blob<T>) -> std::result::Result<encoding::Digest, Self::Error>;
}

impl BlobDigest for SameAsPayload {
    type Error = crate::Error;

    fn digest<T>(blob: &Blob<T>) -> std::result::Result<encoding::Digest, Self::Error> {
        Ok(blob.payload)
    }
}

/// Blobs represent an arbitrary chunk of binary data, usually a file.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Blob<DigestImpl = SameAsPayload> {
    pub payload: encoding::Digest,
    pub size: u64,
    phantom: std::marker::PhantomData<DigestImpl>,
}

impl Blob {
    pub fn new(payload: encoding::Digest, size: u64) -> Self {
        Self {
            payload,
            size,
            phantom: std::marker::PhantomData,
        }
    }

    /// Return the child object of this one in the object DG.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        Vec::new()
    }
}

impl encoding::Encodable for Blob {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut writer, &self.payload)?;
        encoding::write_uint(writer, self.size)?;
        Ok(())
    }
}
impl encoding::Decodable for Blob {
    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Blob {
            payload: encoding::read_digest(&mut reader)?,
            size: encoding::read_uint(reader)?,
            phantom: std::marker::PhantomData,
        })
    }
}

impl Kind for Blob {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::Blob
    }
}

impl<D> Digestible for Blob<D>
where
    D: BlobDigest<Error = crate::Error>,
{
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<encoding::Digest, Self::Error> {
        D::digest(self)
    }
}
