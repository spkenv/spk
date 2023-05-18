// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::io::BufRead;
use std::str::FromStr;
use std::string::ToString;

use itertools::Itertools;

use crate::{encoding, Error, Result};

pub const LEVEL_SEPARATOR: char = '/';

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
#[derive(Clone)]
pub struct Entry<T = ()> {
    pub kind: EntryKind,
    pub object: encoding::Digest,
    pub mode: u32,
    pub size: u64,
    pub entries: std::collections::HashMap<String, Entry<T>>,
    pub user_data: T,
}

impl<T> Default for Entry<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            kind: EntryKind::Tree,
            object: encoding::NULL_DIGEST.into(),
            mode: 0o777,
            size: 0,
            entries: Default::default(),
            user_data: T::default(),
        }
    }
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
            size,
            entries,
            user_data,
        } = self;
        f.debug_struct("Entry")
            .field("kind", kind)
            .field("mode", &format!("{mode:#06o}"))
            .field("size", size)
            .field("object", object)
            .field("entries", entries)
            .field("user_data", user_data)
            .finish()
    }
}

impl<T> PartialEq for Entry<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        // break this apart to create compiler errors when
        // new fields are added
        let Entry {
            kind,
            object,
            mode,
            size,
            entries,
            user_data,
        } = other;
        self.kind == *kind
            && self.mode == *mode
            && self.size == *size
            && self.object == *object
            && self.entries == *entries
            && self.user_data == *user_data
    }
}
impl<T> Eq for Entry<T> where T: Eq {}

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

impl<T> Entry<T> {
    pub fn is_symlink(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFLNK
    }
    pub fn is_dir(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFDIR
    }
    pub fn is_regular_file(&self) -> bool {
        (libc::S_IFMT & self.mode) == libc::S_IFREG
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
            size: self.size,
            entries: self
                .entries
                .into_iter()
                .map(|(n, e)| (n, e.strip_user_data()))
                .collect(),
            user_data: (),
        }
    }
}

impl<T> Entry<T>
where
    T: Clone,
{
    /// Generates the disk usage of a given entry.
    pub fn generate_dir_disk_usage(&self, root_path: &String) -> EntryDiskUsage {
        let mut entry_du = if self.is_dir() {
            EntryDiskUsage::new(format!("{}/", root_path), self.size)
        } else {
            EntryDiskUsage::new(root_path.to_string(), self.size)
        };

        let mut to_iter: HashMap<String, HashMap<String, Entry<T>>> = HashMap::new();
        let initial_entries = self.entries.clone();
        to_iter.insert(root_path.to_string(), initial_entries);

        // Loops through all child entries to obtain the total size
        while !to_iter.is_empty() {
            let mut next_iter: HashMap<String, HashMap<String, Entry<T>>> = HashMap::new();
            for (dir, entries) in to_iter.iter().sorted_by_key(|(k, _)| *k) {
                for (name, entry) in entries.iter().sorted_by_key(|(k, _)| *k) {
                    if entry.is_symlink() {
                        continue;
                    }

                    // Skip dirs and only construct and store absolute path for printing if entry is a blob.
                    if entry.kind.is_blob() {
                        let abs_path =
                            [dir.clone(), name.clone()].join(&LEVEL_SEPARATOR.to_string());
                        entry_du.child_entries.push((entry.size, abs_path));
                    }

                    // Must check if child entries exists for next iteration
                    if !entry.entries.is_empty() {
                        let new_dir =
                            [dir.clone(), name.clone()].join(&LEVEL_SEPARATOR.to_string());
                        next_iter.insert(new_dir, entry.entries.clone());
                    }
                    entry_du.total_size += entry.size;
                }
            }
            to_iter = std::mem::take(&mut next_iter);
        }
        entry_du
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

/// Stores the entry's disk usage data.
/// The child entries are stored in the format of (size, path_to_file).
#[derive(Default, Clone, Debug)]
pub struct EntryDiskUsage {
    pub root: String,
    pub total_size: u64,
    pub child_entries: Vec<(u64, String)>,
}

impl EntryDiskUsage {
    pub fn new(root: String, size: u64) -> Self {
        Self {
            root,
            total_size: size,
            child_entries: Vec::new(),
        }
    }
}
