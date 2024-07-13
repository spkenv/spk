// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::BufRead;
use std::str::FromStr;
use std::string::ToString;

use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./entry_test.rs"]
mod entry_test;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EntryKind {
    /// directory / node
    Tree,
    /// file / leaf with size
    Blob(u64),
    /// removed entry / node or leaf
    Mask,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree => f.write_str("tree"),
            Self::Blob(_) => {
                // This is currently used in encoding and the size is not
                // included.
                f.write_str("file")
            }
            Self::Mask => f.write_str("mask"),
        }
    }
}

impl PartialOrd for EntryKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntryKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.is_tree(), other.is_tree()) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl EntryKind {
    pub fn is_tree(&self) -> bool {
        matches!(self, Self::Tree)
    }
    pub fn is_blob(&self) -> bool {
        matches!(self, Self::Blob(_))
    }
    pub fn is_mask(&self) -> bool {
        matches!(self, Self::Mask)
    }
}

impl FromStr for EntryKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "tree" => Ok(Self::Tree),
            "file" => Ok(Self::Blob(0)),
            "mask" => Ok(Self::Mask),
            kind => Err(format!("invalid entry kind: {kind}").into()),
        }
    }
}

impl From<EntryKind> for spfs_proto::EntryKind {
    fn from(val: EntryKind) -> spfs_proto::EntryKind {
        match val {
            EntryKind::Blob(_) => spfs_proto::EntryKind::Blob,
            EntryKind::Tree => spfs_proto::EntryKind::Tree,
            EntryKind::Mask => spfs_proto::EntryKind::Mask,
        }
    }
}

impl encoding::Encodable for EntryKind {
    type Error = Error;

    fn encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_string(writer, self.to_string().as_ref()).map_err(Error::Encoding)
    }
}

impl encoding::Decodable for EntryKind {
    fn decode(reader: &mut impl BufRead) -> Result<Self> {
        Self::from_str(encoding::read_string(reader)?.as_str())
    }
}

/// An entry in the manifest identifies a directory or file in the tree
///
/// Any associated user data is not considered for comparison, sorting, etc.
#[derive(Clone)]
pub struct Entry<T = ()> {
    pub kind: EntryKind,
    pub object: encoding::Digest,
    pub mode: u32,
    pub entries: std::collections::HashMap<String, Entry<T>>,
    pub user_data: T,
    /// The size associated with non-blob entries.
    pub legacy_size: u64,
}

impl<T> std::fmt::Debug for Entry<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // break this apart to create compiler errors when
        // new fields are added
        let Entry {
            kind,
            object,
            mode,
            entries,
            user_data,
            legacy_size: _,
        } = self;
        f.debug_struct("Entry")
            .field("kind", kind)
            .field("mode", &format!("{mode:#06o}"))
            .field("object", object)
            .field("entries", entries)
            .field("user_data", user_data)
            .finish()
    }
}

impl<T, T2> PartialEq<Entry<T2>> for Entry<T> {
    fn eq(&self, other: &Entry<T2>) -> bool {
        // break this apart to create compiler errors when
        // new fields are added
        let Entry {
            kind,
            object,
            mode,
            entries,
            user_data: _,
            legacy_size: _,
        } = other;
        if self.kind != *kind
            || self.mode != *mode
            || self.size() != other.size()
            || self.object != *object
        {
            return false;
        }
        if self.entries.len() != entries.len() {
            return false;
        }
        for (name, value) in self.entries.iter() {
            let Some(other) = entries.get(name) else {
                return false;
            };
            if value != other {
                return false;
            }
        }
        true
    }
}
impl<T> Eq for Entry<T> {}

impl<T> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self.kind, other.kind) {
            (EntryKind::Tree, EntryKind::Tree) => None,
            (EntryKind::Tree, _) => Some(std::cmp::Ordering::Greater),
            (_, EntryKind::Tree) => Some(std::cmp::Ordering::Less),
            _ => None,
        }
    }
}

impl<T> Entry<T> {
    /// Create an entry that represents a masked file
    pub fn mask_with_data(user_data: T) -> Self {
        Self {
            kind: EntryKind::Mask,
            object: encoding::NULL_DIGEST.into(),
            mode: 0o0777, // for backwards-compatible hashing
            entries: Default::default(),
            user_data,
            legacy_size: 0,
        }
    }

    /// Create an entry that represents an empty symlink
    pub fn empty_symlink_with_data(user_data: T) -> Self {
        Self {
            kind: EntryKind::Blob(0),
            object: encoding::EMPTY_DIGEST.into(),
            mode: 0o0120777,
            entries: Default::default(),
            user_data,
            legacy_size: 0,
        }
    }

    /// Create an entry that represents an empty
    /// directory with fully open permissions
    pub fn empty_dir_with_open_perms_with_data(user_data: T) -> Self {
        Self {
            kind: EntryKind::Tree,
            object: encoding::NULL_DIGEST.into(),
            mode: 0o0040777,
            entries: Default::default(),
            user_data,
            legacy_size: 0,
        }
    }

    /// Create an entry that represents an empty
    /// directory with fully open permissions
    pub fn empty_file_with_open_perms_with_data(user_data: T) -> Self {
        Self {
            kind: EntryKind::Blob(0),
            object: encoding::EMPTY_DIGEST.into(),
            mode: 0o0100777,
            entries: Default::default(),
            user_data,
            legacy_size: 0,
        }
    }

    /// Return the size of the blob; size is meaningless for other entry types.
    #[inline]
    pub fn size(&self) -> u64 {
        match self.kind {
            EntryKind::Blob(size) => size,
            _ => 0,
        }
    }

    /// Return the size of the blob or the legacy size for non-blobs.
    ///
    /// This is required to preserve backwards compatibility with how digests
    /// are calculated for non-blob entries.
    #[inline]
    pub fn size_for_legacy_encode(&self) -> u64 {
        match self.kind {
            EntryKind::Blob(size) => size,
            _ => self.legacy_size,
        }
    }
}

impl<T> Entry<T>
where
    T: Default,
{
    /// Create an entry that represents a masked file
    pub fn mask() -> Self {
        Self::mask_with_data(T::default())
    }

    /// Create an entry that represents an empty symlink
    pub fn empty_symlink() -> Self {
        Self::empty_symlink_with_data(T::default())
    }

    /// Create an entry that represents an empty
    /// directory with fully open permissions
    pub fn empty_dir_with_open_perms() -> Self {
        Self::empty_dir_with_open_perms_with_data(T::default())
    }

    /// Create an entry that represents an empty
    /// directory with fully open permissions
    pub fn empty_file_with_open_perms() -> Self {
        Self::empty_file_with_open_perms_with_data(T::default())
    }
}

impl<T> Entry<T> {
    pub fn is_symlink(&self) -> bool {
        unix_mode::is_symlink(self.mode)
    }
    pub fn is_dir(&self) -> bool {
        unix_mode::is_dir(self.mode)
    }
    pub fn is_regular_file(&self) -> bool {
        unix_mode::is_file(self.mode)
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = super::manifest::ManifestNode<'_, T>> {
        self.entries
            .iter()
            .map(|(name, entry)| super::manifest::ManifestNode {
                path: name.into(),
                entry,
            })
    }

    pub fn strip_user_data(self) -> Entry<()> {
        Entry {
            kind: self.kind,
            object: self.object,
            mode: self.mode,
            entries: self
                .entries
                .into_iter()
                .map(|(n, e)| (n, e.strip_user_data()))
                .collect(),
            user_data: (),
            legacy_size: self.legacy_size,
        }
    }

    /// Clone the provided user data into this entry and any children
    pub fn and_user_data<T1: Clone>(self, user_data: T1) -> Entry<T1> {
        Entry {
            kind: self.kind,
            object: self.object,
            mode: self.mode,
            entries: self
                .entries
                .into_iter()
                .map(|(n, e)| (n, e.and_user_data(user_data.clone())))
                .collect(),
            user_data,
            legacy_size: self.legacy_size,
        }
    }

    /// Map this entry with the given user data, applying
    /// the default value to any children.
    pub fn with_user_data<T1: Default>(self, user_data: T1) -> Entry<T1> {
        Entry {
            kind: self.kind,
            object: self.object,
            mode: self.mode,
            entries: self
                .entries
                .into_iter()
                .map(|(n, e)| (n, e.with_user_data(T1::default())))
                .collect(),
            user_data,
            legacy_size: self.legacy_size,
        }
    }
}

impl<T> Entry<T>
where
    T: Clone,
{
    pub fn update(&mut self, other: &Self) {
        self.kind = other.kind;
        self.object = other.object;
        self.mode = other.mode;
        if !self.kind.is_tree() {
            return;
        }

        for (name, node) in other.entries.iter() {
            if node.kind.is_mask() {
                self.entries.remove(name);
            }

            if let Some(existing) = self.entries.get_mut(name) {
                existing.update(node);
            } else {
                self.entries.insert(name.clone(), node.clone());
            }
        }
    }
}
