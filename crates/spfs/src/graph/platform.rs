// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::Stack;
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
pub struct Platform {
    /// Items in the platform, where the first element is the bottom of the
    /// stack, and may be overridden by later elements higher in the stack
    pub stack: Stack,
}

impl Platform {
    pub fn from_encodable<E, I>(layers: I) -> Result<Self>
    where
        E: encoding::Encodable,
        Error: std::convert::From<E::Error>,
        I: IntoIterator<Item = E>,
    {
        Stack::from_encodable(layers).map(|stack| Self { stack })
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        self.stack.iter_bottom_up().collect()
    }
}

impl Encodable for Platform {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        // use a vec to know the name ahead of time and
        // avoid iterating the stack twice
        let digests = self.stack.iter_bottom_up().collect::<Vec<_>>();
        encoding::write_uint64(&mut writer, digests.len() as u64)?;
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order
        for digest in digests.into_iter().rev() {
            encoding::write_digest(&mut writer, &digest)?;
        }
        Ok(())
    }
}

impl encoding::Decodable for Platform {
    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let num_layers = encoding::read_uint64(&mut reader)?;
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
        }
    }
}

impl<T> FromIterator<T> for Platform
where
    Stack: FromIterator<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            stack: Stack::from_iter(iter),
        }
    }
}
