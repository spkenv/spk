// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::io::BufRead;

use super::Entry;
use crate::encoding;
use crate::encoding::Encodable;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./tree_test.rs"]
mod tree_test;

/// Tree is an ordered collection of entries.
///
/// Only one entry of a given name is allowed at a time.
#[derive(Default, Clone)]
pub struct Tree {
    pub entries: BTreeSet<Entry>,
}

impl Tree {
    pub fn new(entries: impl Iterator<Item = Entry>) -> Self {
        Self {
            entries: entries.collect(),
        }
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<&Entry> {
        for entry in self.entries.iter() {
            if entry.name == name.as_ref() {
                return Some(entry);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    ///Add an entry to this tree.
    ///
    /// # Errors:
    /// - if an entry with the same name exists
    pub fn add(&mut self, entry: Entry) -> Result<()> {
        if !self.entries.insert(entry) {
            Err(Error::String("Tree entry already exists".to_owned()))
        } else {
            Ok(())
        }
    }

    pub fn update(&mut self, entry: Entry) -> Result<()> {
        let _ = self.remove(entry.name.as_str());
        self.add(entry)
    }

    pub fn remove<S: AsRef<str>>(&mut self, name: S) -> Option<&Entry> {
        for entry in self.entries.iter() {
            if entry.name == name.as_ref() {
                return Some(entry);
            }
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }
}

impl std::fmt::Debug for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Tree {{ {:?} }}", self.digest().unwrap()))
    }
}

impl PartialEq for Tree {
    fn eq(&self, other: &Self) -> bool {
        self.digest()
            .unwrap_or_else(|_| encoding::NULL_DIGEST.into())
            == other
                .digest()
                .unwrap_or_else(|_| encoding::NULL_DIGEST.into())
    }
}
impl Eq for Tree {}

impl encoding::Encodable for Tree {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_uint(&mut writer, self.len() as u64)?;
        let mut entries: Vec<_> = self.entries.iter().collect();
        // this is not the default sort mode for entries but
        // matches the existing compatible encoding order
        entries.sort_unstable_by_key(|e| &e.name);
        for entry in entries.into_iter() {
            entry.encode(writer)?;
        }
        Ok(())
    }
}

impl encoding::Decodable for Tree {
    fn decode(mut reader: &mut impl BufRead) -> Result<Self> {
        let mut tree = Tree {
            entries: Default::default(),
        };
        let entry_count = encoding::read_uint(&mut reader)?;
        for _ in 0..entry_count {
            tree.entries.insert(Entry::decode(reader)?);
        }
        Ok(tree)
    }
}
