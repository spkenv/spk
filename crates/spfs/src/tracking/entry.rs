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
    // Walks through all entries from the given root entry and generates its size and the file path.
    pub fn calculate_size_of_entries(
        &self,
        root_dir: String,
        _disk_usage: &HashMap<String, Entry<T>>,
    ) -> Vec<(u64, String)> {
        let mut result: Vec<(u64, String)> = Vec::new();
        if self.kind.is_blob() {
            result.push((self.size, root_dir.to_string()))
        }

        for (name, entry) in self.entries.iter() {
            if entry.is_symlink() {
                continue;
            }

            let abs_path = match root_dir.is_empty() {
                true => name.to_string(),
                false => [root_dir.to_string(), name.to_string()].join(&LEVEL_SEPARATOR.to_string()),
            };

            // Skip dirs and only construct and store absolute path for printing if entry is a blob.
            if entry.kind.is_blob() {
                result.push((entry.size, abs_path.to_string()));
            }

            // Calculate entry sizes for child entries.
            if !entry.entries.is_empty() {
                result.extend(entry.calculate_size_of_entries(abs_path.to_string(), _disk_usage));
            }
        }
        result
    }

    /// Generates the disk usage of a given entry.
    pub fn generate_entry_disk_usage(&self, root: &String) -> EntryDiskUsage {
        let mut entry_du = EntryDiskUsage::new(root.to_string());
        entry_du.kind = self.kind;
        entry_du
            .child_entries
            .extend(self.calculate_size_of_entries(root.to_string(), &self.entries));

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
#[derive(Clone, Debug)]
pub struct EntryDiskUsage {
    pub root: String,
    pub kind: EntryKind,
    pub total_size: u64,
    pub pkg_info: String,
    pub deprecated: bool,
    pub child_entries: Vec<(u64, String)>,
}

impl EntryDiskUsage {
    pub fn new(root: String) -> Self {
        Self {
            root,
            kind: EntryKind::Tree,
            total_size: 0,
            pkg_info: String::new(),
            deprecated: false,
            child_entries: Vec::new(),
        }
    }

    fn update_total_size_of_entries(&mut self) {
        for (size, _) in self.child_entries.iter() {
            self.total_size += size;
        }
    }

    fn get_file_paths_of_child_entries(&self) -> Vec<String> {
        let mut file_paths: Vec<String> = Vec::new();
        for (_, path) in self.child_entries.iter() {
            file_paths.push(path.to_string());
        }
        file_paths
    }

    fn get_entry_names_in_root(&self) -> Vec<String> {
        let mut entries: Vec<String> = Vec::new();
        let path_length = match self.root.is_empty() {
            true => 0,
            false => self.root.split(LEVEL_SEPARATOR).collect_vec().len(),
        };
        if self.kind.is_tree() {
            for (_, path) in self.child_entries.iter() {
                let mut sub_path = path.split(LEVEL_SEPARATOR).collect_vec();
                sub_path.retain(|c| !c.is_empty());
                entries.push(sub_path[..=path_length].join(&LEVEL_SEPARATOR.to_string()));
            }
        }

        entries.dedup();
        entries
    }

    /// Formats the entries in child_entries to output.
    pub fn convert_child_entries_for_output(&self) -> HashMap<(String, bool), u64> {
        let mut formatted_entries: HashMap<(String, bool), u64> = HashMap::default();
        for (size, path) in self.child_entries.iter() {
            formatted_entries.insert((format!("{}/{path}", self.pkg_info), self.deprecated), *size);
        }
        formatted_entries
    }

    /// Groups the entry and sums the sizes together.
    /// If the input ends with a level separator '/' this means we need to
    /// output and sum the sizes of the entries contained inside the root dir.
    /// Else, if the input does not end with a level separator we group and sum the
    /// sizes by the root dir.
    pub fn group_entries(&self, group_by_dirs_in_root: bool) -> HashMap<(String, bool), u64> {
        let mut sum_by_dir: HashMap<(String, bool), u64> = HashMap::default();
        match group_by_dirs_in_root {
            false => {
                for (size, path) in self.child_entries.iter() {
                    if path.contains(&self.root) {
                        let abs_path = match self.kind.is_tree() {
                            true => format!("{}/{}/", self.pkg_info, self.root),
                            false => format!("{}/{}", self.pkg_info, self.root),
                        };
                        sum_by_dir
                            .entry((abs_path.to_string(), self.deprecated))
                            .and_modify(|s| *s += size)
                            .or_insert(*size);
                    }
                }
            }
            true => {
                let entries_in_root = self.get_entry_names_in_root();
                for entry in entries_in_root.iter() {
                    let (sizes, paths): (Vec<u64>, Vec<String>) = self
                        .child_entries
                        .iter()
                        .cloned()
                        .filter(|(_, p)| p.contains(entry))
                        .unzip();
                    let abs_path = match paths.contains(entry) {
                        true => format!("{}/{entry}", self.pkg_info),
                        false => format!("{}/{entry}/", self.pkg_info),
                    };
                    let total_size = sizes.into_iter().sum();
                    sum_by_dir
                        .entry((abs_path, self.deprecated))
                        .and_modify(|s| *s += total_size)
                        .or_insert(total_size);
                }
            }
        }
        sum_by_dir
    }

    /// Calculates the total size of the entry.
    pub fn calculate_total_size(
        &mut self,
        component_entries: &Vec<EntryDiskUsage>,
        count_links: bool,
    ) {
        match component_entries.is_empty() {
            false => {
                let curr_comp_file_paths = self.get_file_paths_of_child_entries();
                for entry_du in component_entries.iter() {
                    for (size, path) in entry_du.child_entries.iter() {
                        if !curr_comp_file_paths.contains(path) || count_links {
                            self.total_size += size
                        }
                    }
                }
            }
            true => self.update_total_size_of_entries(),
        }
    }
}
