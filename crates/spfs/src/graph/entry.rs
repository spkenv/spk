// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;

use super::{DigestFromEncode, EncodeDigest};
use crate::{encoding, tracking, Error, Result};

#[cfg(test)]
#[path = "./entry_test.rs"]
mod entry_test;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Entry<DigestImpl = DigestFromEncode> {
    pub object: encoding::Digest,
    pub kind: tracking::EntryKind,
    pub mode: u32,
    pub size: u64,
    pub name: String,
    phantom: std::marker::PhantomData<DigestImpl>,
}

impl Entry {
    pub fn new(
        object: encoding::Digest,
        kind: tracking::EntryKind,
        mode: u32,
        size: u64,
        name: String,
    ) -> Self {
        Self {
            object,
            kind,
            mode,
            size,
            name,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn from<T>(name: String, entry: &tracking::Entry<T>) -> Self {
        Self {
            object: entry.object,
            kind: entry.kind,
            mode: entry.mode,
            size: entry.size,
            name,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn is_symlink(&self) -> bool {
        unix_mode::is_symlink(self.mode)
    }

    pub fn is_dir(&self) -> bool {
        unix_mode::is_dir(self.mode)
    }

    pub fn is_regular_file(&self) -> bool {
        unix_mode::is_file(self.mode)
    }
}

impl std::fmt::Display for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{:06o} {:?} {} {}",
            self.mode, self.kind, self.name, self.object
        ))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.kind == other.kind {
            self.name.cmp(&other.name)
        } else {
            self.kind.cmp(&other.kind)
        }
    }
}

impl<D> encoding::Encodable for Entry<D> {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut writer, &self.object)?;
        self.kind.encode(&mut writer)?;
        encoding::write_uint(&mut writer, self.mode as u64)?;
        encoding::write_uint(&mut writer, self.size)?;
        encoding::write_string(writer, self.name.as_str())?;
        Ok(())
    }
}
impl encoding::Decodable for Entry {
    fn decode(mut reader: &mut impl BufRead) -> Result<Self> {
        Ok(Entry {
            object: encoding::read_digest(&mut reader)?,
            kind: tracking::EntryKind::decode(&mut reader)?,
            mode: encoding::read_uint(&mut reader)? as u32,
            size: encoding::read_uint(&mut reader)?,
            name: encoding::read_string(reader)?,
            phantom: std::marker::PhantomData,
        })
    }
}

impl<D> encoding::Digestible for Entry<D>
where
    D: EncodeDigest<Error = crate::Error>,
{
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<encoding::Digest, Self::Error> {
        D::digest(self)
    }
}
