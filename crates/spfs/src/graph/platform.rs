// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::encoding;
use crate::encoding::Encodable;
use crate::Result;

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
    pub stack: Vec<encoding::Digest>,
}

impl Platform {
    pub fn new<E, I>(layers: I) -> Result<Self>
    where
        E: encoding::Encodable,
        I: Iterator<Item = E>,
    {
        let mut platform = Self { stack: Vec::new() };
        for item in layers {
            platform.stack.push(item.digest()?);
        }
        Ok(platform)
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        self.stack.to_vec()
    }
}

impl Encodable for Platform {
    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_uint(&mut writer, self.stack.len() as u64)?;
        for digest in self.stack.iter() {
            encoding::write_digest(&mut writer, digest)?;
        }
        Ok(())
    }
}

impl encoding::Decodable for Platform {
    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let num_layers = encoding::read_uint(&mut reader)?;
        let mut platform = Self {
            stack: Vec::with_capacity(num_layers as usize),
        };
        for _ in 0..num_layers {
            platform.stack.push(encoding::read_digest(&mut reader)?)
        }
        Ok(platform)
    }
}
