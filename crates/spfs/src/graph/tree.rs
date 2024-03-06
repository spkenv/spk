// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;

use super::{Entry, HasKind, ObjectKind};
use crate::encoding::prelude::*;
use crate::{encoding, Result};

#[cfg(test)]
#[path = "./tree_test.rs"]
mod tree_test;

// Tree is an ordered collection of entries.
//
// Only one entry of a given name is allowed at a time.
#[derive(Copy, Clone)]
pub struct Tree<'buf>(pub(super) spfs_proto::Tree<'buf>);

impl<'buf> From<spfs_proto::Tree<'buf>> for Tree<'buf> {
    fn from(value: spfs_proto::Tree<'buf>) -> Self {
        Self(value)
    }
}

impl<'buf> Tree<'buf> {
    pub fn entries(&self) -> impl Iterator<Item = Entry<'buf>> {
        self.0.entries().iter().map(Into::into)
    }

    pub fn into_entries(self) -> impl Iterator<Item = Entry<'buf>> {
        self.0.entries().iter().map(Into::into)
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<Entry<'_>> {
        self.entries().find(|entry| entry.name() == name.as_ref())
    }

    pub(super) fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        let mut entries: Vec<_> = self.entries().collect();
        encoding::write_uint64(&mut writer, entries.len() as u64)?;
        // this is not the default sort mode for entries but
        // matches the existing compatible encoding order
        entries.sort_unstable_by_key(|e| e.name());
        for entry in entries.into_iter() {
            entry.legacy_encode(writer)?;
        }
        Ok(())
    }

    pub(super) fn legacy_decode<'builder>(
        builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
        mut reader: &mut impl BufRead,
    ) -> Result<flatbuffers::WIPOffset<spfs_proto::Tree<'builder>>> {
        let mut entries = Vec::new();
        let entry_count = encoding::read_uint64(&mut reader)?;
        for _ in 0..entry_count {
            entries.push(Entry::legacy_decode(builder, reader)?);
        }
        let entries = builder.create_vector(&entries);
        Ok(spfs_proto::Tree::create(
            builder,
            &spfs_proto::TreeArgs {
                entries: Some(entries),
            },
        ))
    }
}

impl<'buf> encoding::Digestible for Tree<'buf> {
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<spfs_proto::Digest, Self::Error> {
        let mut hasher = encoding::Hasher::new_sync();
        self.legacy_encode(&mut hasher)?;
        Ok(hasher.digest())
    }
}

impl<'buf> std::fmt::Debug for Tree<'buf> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Tree {{ {:?} }}", self.digest().unwrap()))
    }
}

impl<'buf> HasKind for Tree<'buf> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        ObjectKind::Tree
    }
}

/// A wrapper type that holds an owned buffer to an [`Tree`].
///
/// Trees are usually only constructed as part of a larger
/// type, such as a [`super::Manifest`], but for testing it helps
/// to be able to create one on its own.
#[cfg(test)]
pub struct TreeBuf(Box<[u8]>);

#[cfg(test)]
impl TreeBuf {
    pub fn build(entries: Vec<super::entry::EntryBuf>) -> Self {
        crate::graph::BUILDER.with_borrow_mut(|builder| {
            let entries = entries
                .into_iter()
                .map(|entry| {
                    let entry = entry.as_entry();
                    let name = builder.create_string(entry.name());
                    spfs_proto::Entry::create(
                        builder,
                        &spfs_proto::EntryArgs {
                            kind: entry.kind().into(),
                            object: Some(entry.object()),
                            mode: entry.mode(),
                            size_: entry.size_for_legacy_encode(),
                            name: Some(name),
                        },
                    )
                })
                .collect::<Vec<_>>();
            let entries = builder.create_vector(&entries);
            let tree = spfs_proto::Tree::create(
                builder,
                &spfs_proto::TreeArgs {
                    entries: Some(entries),
                },
            );
            builder.finish_minimal(tree);
            let bytes = builder.finished_data().into();
            builder.reset();
            Self(bytes)
        })
    }

    pub fn as_tree(&self) -> Tree<'_> {
        let e =
            flatbuffers::root::<spfs_proto::Tree<'_>>(&self.0[..]).expect("valid internal buffer");
        Tree(e)
    }
}
