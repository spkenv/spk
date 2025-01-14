// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::BufRead;

use encoding::prelude::*;
use spfs_proto::EntryArgs;

use crate::{encoding, tracking, Result};

#[cfg(test)]
#[path = "./entry_test.rs"]
mod entry_test;

/// Entry represents one item in the file system, such as
/// a file or directory.
#[derive(Copy, Clone)]
pub struct Entry<'buf>(pub(super) spfs_proto::Entry<'buf>);

impl std::fmt::Debug for Entry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("name", &self.name())
            .field("kind", &self.kind())
            .field("mode", &self.mode())
            .field("size", &self.size())
            .field("object", self.object())
            .finish()
    }
}

impl<'buf> From<spfs_proto::Entry<'buf>> for Entry<'buf> {
    fn from(value: spfs_proto::Entry<'buf>) -> Self {
        Self(value)
    }
}

impl<'buf> Entry<'buf> {
    /// Construct a valid entry from its component parts
    #[allow(clippy::too_many_arguments)]
    pub fn build<'fbb>(
        builder: &mut flatbuffers::FlatBufferBuilder<'fbb>,
        name: &str,
        kind: tracking::EntryKind,
        mode: u32,
        size: u64,
        object: &encoding::Digest,
    ) -> flatbuffers::WIPOffset<spfs_proto::Entry<'fbb>> {
        let name = builder.create_string(name);
        spfs_proto::Entry::create(
            builder,
            &EntryArgs {
                name: Some(name),
                kind: kind.into(),
                mode,
                size_: size,
                object: Some(object),
            },
        )
    }

    pub fn from<'fbb, T>(
        builder: &mut flatbuffers::FlatBufferBuilder<'fbb>,
        name: &str,
        entry: &tracking::Entry<T>,
    ) -> flatbuffers::WIPOffset<spfs_proto::Entry<'fbb>> {
        Self::build(
            builder,
            name,
            entry.kind,
            entry.mode,
            entry.size_for_legacy_encode(),
            &entry.object,
        )
    }

    #[inline]
    pub fn name(&self) -> &'buf str {
        self.0.name()
    }

    pub fn kind(&self) -> tracking::EntryKind {
        match self.0.kind() {
            spfs_proto::EntryKind::Blob => tracking::EntryKind::Blob(self.0.size_()),
            spfs_proto::EntryKind::Tree => tracking::EntryKind::Tree,
            spfs_proto::EntryKind::Mask => tracking::EntryKind::Mask,
            _ => unreachable!("internally valid entry buffer"),
        }
    }

    #[inline]
    pub fn mode(&self) -> u32 {
        self.0.mode()
    }

    #[inline]
    pub fn size(&self) -> u64 {
        match self.0.kind() {
            spfs_proto::EntryKind::Blob => self.0.size_(),
            _ => 0,
        }
    }

    #[inline]
    pub fn object(&self) -> &'buf encoding::Digest {
        self.0.object()
    }

    #[inline]
    pub fn is_symlink(&self) -> bool {
        unix_mode::is_symlink(self.mode())
    }

    #[inline]
    pub fn is_dir(&self) -> bool {
        unix_mode::is_dir(self.mode())
    }

    #[inline]
    pub fn is_regular_file(&self) -> bool {
        unix_mode::is_file(self.mode())
    }

    /// Return the size of the blob or the legacy size for non-blobs.
    ///
    /// This is required to preserve backwards compatibility with how digests
    /// are calculated for non-blob entries.
    #[inline]
    pub fn size_for_legacy_encode(&self) -> u64 {
        self.0.size_()
    }
}

impl std::fmt::Display for Entry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{:06o} {:?} {} {}",
            self.mode(),
            self.kind(),
            self.name(),
            self.object()
        ))
    }
}

impl<'buf2> PartialEq<Entry<'buf2>> for Entry<'_> {
    fn eq(&self, other: &Entry<'buf2>) -> bool {
        self.0 == other.0
    }
}

impl Eq for Entry<'_> {}

impl<'buf2> PartialOrd<Entry<'buf2>> for Entry<'_> {
    fn partial_cmp(&self, other: &Entry<'buf2>) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'buf> Ord for Entry<'buf> {
    fn cmp(&self, other: &Entry<'buf>) -> std::cmp::Ordering {
        // Note that the Entry's size does not factor into the comparison.
        if self.0.kind() == other.0.kind() {
            self.name().cmp(other.name())
        } else {
            self.kind().cmp(&other.kind())
        }
    }
}

impl encoding::Digestible for Entry<'_> {
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<spfs_proto::Digest, Self::Error> {
        let mut hasher = encoding::Hasher::new_sync();
        self.legacy_encode(&mut hasher)?;
        Ok(hasher.digest())
    }
}

impl Entry<'_> {
    pub(super) fn digest_encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut *writer, self.object())?;
        self.kind().encode(&mut *writer)?;
        encoding::write_uint64(&mut *writer, self.mode() as u64)?;
        encoding::write_uint64(&mut *writer, self.size_for_legacy_encode())?;
        encoding::write_string(writer, self.name())?;
        Ok(())
    }

    pub(super) fn legacy_encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut *writer, self.object())?;
        self.kind().encode(&mut *writer)?;
        encoding::write_uint64(&mut *writer, self.mode() as u64)?;
        encoding::write_uint64(&mut *writer, self.size_for_legacy_encode())?;
        encoding::write_string(writer, self.name())?;
        Ok(())
    }

    pub(super) fn legacy_decode<'builder>(
        builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
        reader: &mut impl BufRead,
    ) -> Result<flatbuffers::WIPOffset<spfs_proto::Entry<'builder>>> {
        // fields in the same order as above
        let object = encoding::read_digest(&mut *reader)?;
        let mut kind = tracking::EntryKind::decode(&mut *reader)?;
        let mode = encoding::read_uint64(&mut *reader)? as u32;
        let size = encoding::read_uint64(&mut *reader)?;
        let name = encoding::read_string(reader)?;
        if kind.is_blob() {
            kind = tracking::EntryKind::Blob(size);
        }
        Ok(Self::build(builder, &name, kind, mode, size, &object))
    }
}

/// A wrapper type that holds an owned buffer to an [`Entry`].
///
/// Entries are usually only constructed as part of a larger
/// type, such as a [`super::Manifest`], but for testing it helps
/// to be able to create one on its own.
#[cfg(test)]
pub struct EntryBuf(Box<[u8]>);

#[cfg(test)]
impl EntryBuf {
    pub fn build(
        name: &str,
        kind: tracking::EntryKind,
        mode: u32,
        object: &encoding::Digest,
    ) -> Self {
        crate::graph::BUILDER.with_borrow_mut(|builder| {
            let name = builder.create_string(name);
            let e = spfs_proto::Entry::create(
                builder,
                &EntryArgs {
                    kind: kind.into(),
                    object: Some(object),
                    mode,
                    size_: {
                        match kind {
                            tracking::EntryKind::Blob(size) => size,
                            _ => 0,
                        }
                    },
                    name: Some(name),
                },
            );
            builder.finish_minimal(e);
            let bytes = builder.finished_data().into();
            builder.reset();
            Self(bytes)
        })
    }

    pub fn build_with_legacy_size(
        name: &str,
        kind: tracking::EntryKind,
        mode: u32,
        size: u64,
        object: &encoding::Digest,
    ) -> Self {
        crate::graph::BUILDER.with_borrow_mut(|builder| {
            let name = builder.create_string(name);
            let e = spfs_proto::Entry::create(
                builder,
                &EntryArgs {
                    kind: kind.into(),
                    object: Some(object),
                    mode,
                    size_: {
                        match kind {
                            tracking::EntryKind::Blob(size) => size,
                            _ => size,
                        }
                    },
                    name: Some(name),
                },
            );
            builder.finish_minimal(e);
            let bytes = builder.finished_data().into();
            builder.reset();
            Self(bytes)
        })
    }

    pub fn as_entry(&self) -> Entry<'_> {
        let e =
            flatbuffers::root::<spfs_proto::Entry<'_>>(&self.0[..]).expect("valid internal buffer");
        Entry(e)
    }
}
