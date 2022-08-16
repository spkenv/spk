// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;
use std::str::FromStr;
use std::string::ToString;

use crate::encoding;
use crate::{Error, Result};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EntryKind {
    /// directory / node
    Tree,
    /// file / leaf
    Blob,
    /// removed entry / node or leaf
    Mask,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree => f.write_str("tree"),
            Self::Blob => f.write_str("file"),
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
        matches!(self, Self::Blob)
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
            "file" => Ok(Self::Blob),
            "mask" => Ok(Self::Mask),
            kind => Err(format!("invalid entry kind: {kind}").into()),
        }
    }
}

impl encoding::Encodable for EntryKind {
    fn encode(&self, writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_string(writer, self.to_string().as_ref())
    }
}
impl encoding::Decodable for EntryKind {
    fn decode(reader: &mut impl BufRead) -> Result<Self> {
        Self::from_str(encoding::read_string(reader)?.as_str())
    }
}

#[derive(Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub object: encoding::Digest,
    pub mode: u32,
    pub size: u64,
    pub entries: std::collections::HashMap<String, Entry>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            kind: EntryKind::Tree,
            object: encoding::NULL_DIGEST.into(),
            mode: 0o777,
            size: 0,
            entries: Default::default(),
        }
    }
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("kind", &self.kind)
            .field("mode", &format!("{:#06o}", self.mode))
            .field("size", &self.size)
            .field("object", &self.object)
            .field("entries", &self.entries)
            .finish()
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.mode == other.mode
            && self.size == other.size
            && self.object == other.object
    }
}
impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self.kind, other.kind) {
            (EntryKind::Tree, EntryKind::Tree) => None,
            (EntryKind::Tree, _) => Some(std::cmp::Ordering::Greater),
            (_, EntryKind::Tree) => Some(std::cmp::Ordering::Less),
            _ => None,
        }
    }
}

impl Entry {
    pub fn is_symlink(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFLNK
    }
    pub fn is_dir(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFDIR
    }
    pub fn is_regular_file(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFREG
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = super::manifest::ManifestNode<'_>> {
        self.entries
            .iter()
            .map(|(name, entry)| super::manifest::ManifestNode {
                path: name.into(),
                entry,
            })
    }

    pub fn update(&mut self, other: &Self) {
        self.kind = other.kind;
        self.object = other.object;
        self.mode = other.mode;
        if !self.kind.is_tree() {
            self.size = other.size;
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
        self.size = self.entries.len() as u64;
    }
}
