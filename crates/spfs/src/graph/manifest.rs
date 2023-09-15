// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

use encoding::Decodable;

use super::{Entry, Tree};
use crate::encoding::Encodable;
use crate::{encoding, tracking, Error, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct Manifest {
    root: Tree,
    // because manifests are encoded - the ordering of trees are important
    // to maintain in order to create consistent hashing
    tree_order: Vec<encoding::Digest>,
    trees: BTreeMap<encoding::Digest, Tree>,
}

impl<T> From<&tracking::Manifest<T>> for Manifest
where
    T: std::cmp::Eq + std::cmp::PartialEq,
{
    fn from(source: &tracking::Manifest<T>) -> Self {
        Self::from(source.root())
    }
}

impl<T> From<&tracking::Entry<T>> for Manifest
where
    T: std::cmp::Eq + std::cmp::PartialEq,
{
    fn from(source: &tracking::Entry<T>) -> Self {
        let mut manifest = Self::default();
        let mut root = Tree::default();

        let mut entries: Vec<_> = source.iter_entries().collect();
        entries.sort_unstable();
        for node in entries {
            let converted = match node.entry.kind {
                tracking::EntryKind::Tree => {
                    let sub = Self::from(node.entry);
                    for tree in sub.iter_trees() {
                        manifest
                            .insert_tree(tree.clone())
                            .expect("should not fail to insert tree entry");
                    }
                    Entry {
                        object: sub.root.digest().unwrap(),
                        kind: node.entry.kind,
                        mode: node.entry.mode,
                        size: node.entry.size,
                        name: node.path.to_string(),
                    }
                }
                _ => Entry::from(node.path.to_string(), node.entry),
            };
            root.entries.insert(converted);
        }
        manifest.root = root;
        manifest
    }
}

impl Manifest {
    /// Create a new manifest with the given tree as the root.
    ///
    /// It's very possible to create an internally inconsistent manifest
    /// this way, so ensure that any additional tree entries in the given
    /// root tree are subsequently inserted into the created manifest
    pub(crate) fn new(root: Tree) -> Self {
        Self {
            root,
            ..Default::default()
        }
    }

    /// Return the root tree object of this manifest.
    pub fn root(&self) -> &Tree {
        &self.root
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        let mut children = BTreeSet::new();
        for tree in self.iter_trees() {
            for entry in tree.entries.iter() {
                if let tracking::EntryKind::Blob = entry.kind {
                    children.insert(entry.object);
                }
            }
        }
        children.into_iter().collect()
    }

    /// Add a tree to be tracked in this manifest, returning
    /// it if the same tree already exists.
    pub(crate) fn insert_tree(&mut self, tree: Tree) -> Result<Option<Tree>> {
        let digest = tree.digest()?;
        if let Some(tree) = self.trees.insert(digest, tree) {
            Ok(Some(tree))
        } else {
            self.tree_order.push(digest);
            Ok(None)
        }
    }

    pub fn get_tree<'a>(&'a self, digest: &encoding::Digest) -> Option<&'a Tree> {
        match self.trees.get(digest) {
            None => {
                if digest == &self.root.digest().unwrap() {
                    Some(&self.root)
                } else {
                    None
                }
            }
            some => some,
        }
    }

    /// Iterate all of the trees in this manifest.
    ///
    /// Will panic if this manifest is internally inconsistent, though this
    /// would point to a programming error or bug.
    pub fn iter_trees(&self) -> impl Iterator<Item = &Tree> {
        std::iter::once(&self.root).chain(self.tree_order.iter().map(|digest| {
            self.trees
                .get(digest)
                .expect("manifest is internally inconsistent (missing indexed tree)")
        }))
    }

    /// Iterate all of the entries in this manifest.
    pub fn iter_entries(&self) -> impl Iterator<Item = &Entry> {
        self.iter_trees().flat_map(|t| t.entries.iter())
    }

    /// Convert this manifest into a more workable form for editing.
    pub fn to_tracking_manifest(&self) -> tracking::Manifest {
        let mut root = tracking::Entry::empty_dir_with_open_perms();

        fn iter_tree(source: &Manifest, tree: &Tree, parent: &mut tracking::Entry) {
            for entry in tree.entries.iter() {
                let mut new_entry = tracking::Entry {
                    kind: entry.kind,
                    mode: entry.mode,
                    size: entry.size,
                    entries: Default::default(),
                    object: entry.object,
                    user_data: (),
                };
                if let tracking::EntryKind::Tree = entry.kind {
                    new_entry.object = encoding::NULL_DIGEST.into();
                    iter_tree(
                        source,
                        source
                            .get_tree(&entry.object)
                            .expect("manifest is internally inconsistent (missing child tree)"),
                        &mut new_entry,
                    )
                }
                parent.entries.insert(entry.name.clone(), new_entry);
            }
        }

        iter_tree(self, &self.root, &mut root);
        tracking::Manifest::new(root)
    }
}

impl Encodable for Manifest {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        self.root().encode(&mut writer)?;
        encoding::write_uint(&mut writer, self.tree_order.len() as u64)?;
        for digest in &self.tree_order {
            match self.trees.get(digest) {
                Some(tree) => tree.encode(writer)?,
                None => {
                    return Err("manifest is internally inconsistent (missing indexed tree)".into())
                }
            }
        }
        Ok(())
    }
}

impl Decodable for Manifest {
    fn decode(mut reader: &mut impl BufRead) -> Result<Self> {
        let mut manifest = Manifest {
            root: Tree::decode(&mut reader)?,
            ..Default::default()
        };
        let num_trees = encoding::read_uint(&mut reader)?;
        for _ in 0..num_trees {
            let tree = Tree::decode(reader)?;
            manifest.insert_tree(tree)?;
        }
        Ok(manifest)
    }
}
