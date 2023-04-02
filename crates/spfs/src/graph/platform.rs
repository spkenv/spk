// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::object::Kind;
use super::{DigestFromEncode, DigestFromKindAndEncode, KindAndEncodeDigest, ObjectKind, Stack};
use crate::encoding::Encodable;
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./platform_test.rs"]
mod platform_test;

/// Platforms represent a predetermined collection of layers.
///
/// Platforms capture an entire runtime stack of layers or other platforms
/// as a single, identifiable object which can be applied/installed to
/// future runtimes.
#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct Platform<DigestImpl = DigestFromKindAndEncode> {
    /// Items in the platform, where the first element is the bottom of the
    /// stack, and may be overridden by later elements higher in the stack
    pub stack: Stack,
    phantom: std::marker::PhantomData<DigestImpl>,
}

impl<D> Platform<D> {
    pub fn new(stack: Stack) -> Self {
        Self {
            stack,
            phantom: std::marker::PhantomData,
        }
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        self.stack.iter_bottom_up().collect()
    }

    pub fn from_digestible<E, I>(layers: I) -> Result<Self>
    where
        E: encoding::Digestible,
        Error: std::convert::From<<E as encoding::Digestible>::Error>,
        I: IntoIterator<Item = E>,
    {
        Stack::from_digestible(layers).map(|stack| Self {
            stack,
            phantom: std::marker::PhantomData,
        })
    }
}

impl<D> Encodable for Platform<D> {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        // use a vec to know the name ahead of time and
        // avoid iterating the stack twice
        let digests = self.stack.iter_bottom_up().collect::<Vec<_>>();
        encoding::write_uint(&mut writer, digests.len() as u64)?;
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order
        for digest in digests.into_iter().rev() {
            encoding::write_digest(&mut writer, &digest)?;
        }
        Ok(())
    }
}

impl<D> encoding::Decodable for Platform<D> {
    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let num_layers = encoding::read_uint(&mut reader)?;
        let mut layers = Vec::with_capacity(num_layers as usize);
        for _ in 0..num_layers {
            layers.push(encoding::read_digest(&mut reader)?);
        }
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order
        Ok(Self::from_iter(layers.into_iter().rev()))
    }
}

impl<T> From<T> for Platform
where
    T: Into<Stack>,
{
    fn from(value: T) -> Self {
        Self {
            stack: value.into(),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<T, D> FromIterator<T> for Platform<D>
where
    Stack: FromIterator<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            stack: Stack::from_iter(iter),
            phantom: std::marker::PhantomData,
        }
    }
}

impl Kind for Platform<DigestFromEncode> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::PlatformV1
    }
}

impl Kind for Platform<DigestFromKindAndEncode> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::PlatformV2
    }
}

impl<D> encoding::Digestible for Platform<D>
where
    Self: Kind,
    D: KindAndEncodeDigest<Error = crate::Error>,
{
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<encoding::Digest, Self::Error> {
        D::digest(self)
    }
}
