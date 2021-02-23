use std::collections::{BTreeMap, BTreeSet};

use super::{Entry, Tree};
use crate::encoding::Encodable;
use crate::Result;
use crate::{encoding, tracking};
use encoding::Decodable;

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

#[derive(Debug, Eq, PartialEq)]
pub struct Manifest {
    root: encoding::Digest,
    // because manifests are encoded - the ordering of trees are important
    // to maintain in order to create consistent hashing
    tree_order: Vec<encoding::Digest>,
    trees: BTreeMap<encoding::Digest, Tree>,
}

impl Default for Manifest {
    fn default() -> Self {
        let mut manifest = Manifest {
            root: encoding::NULL_DIGEST.into(),
            trees: Default::default(),
            tree_order: Default::default(),
        };
        // add the default empty tree to make this manifest internally coherent
        manifest
            .insert_tree(Tree::default())
            .expect("should never fail on first entry");
        manifest
    }
}

impl From<&tracking::Manifest> for Manifest {
    fn from(source: &tracking::Manifest) -> Self {
        Self::from(source.root())
    }
}

impl From<&tracking::Entry> for Manifest {
    fn from(source: &tracking::Entry) -> Self {
        let mut manifest = Self::default();
        let mut root = Tree::default();
        for (name, entry) in source.entries.iter() {
            let converted = match entry.kind {
                tracking::EntryKind::Tree => {
                    let sub = Self::from(entry);
                    for (_, tree) in sub.trees {
                        manifest
                            .insert_tree(tree)
                            .expect("should not fail to insert tree entry");
                    }
                    Entry {
                        object: sub.root,
                        kind: entry.kind,
                        mode: entry.mode,
                        size: entry.size,
                        name: name.clone(),
                    }
                }
                _ => Entry::from(name.clone(), &entry),
            };
            root.entries.insert(converted);
        }
        manifest.root = root.digest().expect("failed to hash root entry");
        manifest
            .insert_tree(root)
            .expect("failed to insert final root entry");
        manifest
    }
}

impl Manifest {
    /// Return the root tree object of this manifest.
    pub fn root<'a>(&'a self) -> &'a Tree {
        self.trees
            .get(&self.root)
            .expect("manifest is internally inconsistent")
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        let mut children = BTreeSet::new();
        for tree in self.trees.values() {
            for entry in tree.entries.iter() {
                if let tracking::EntryKind::Blob = entry.kind {
                    children.insert(entry.object.clone());
                }
            }
        }
        return children.into_iter().collect();
    }

    /// Add a tree to be tracked in this manifest, returning
    /// it if the same tree already exists.
    fn insert_tree(&mut self, tree: Tree) -> Result<Option<Tree>> {
        let digest = tree.digest()?;
        if let Some(tree) = self.trees.insert(digest, tree) {
            Ok(Some(tree))
        } else {
            self.tree_order.push(digest);
            Ok(None)
        }
    }

    /// Iterate all of the entries in this manifest.
    pub fn iter_entries<'a>(&'a self) -> Vec<&'a Entry> {
        let mut children = Vec::new();
        for tree in self.trees.values() {
            for entry in tree.entries.iter() {
                children.push(entry);
            }
        }
        children
    }

    /// Unlock creates a tracking manifest that is more workable
    pub fn unlock(&self) -> tracking::Manifest {
        let mut root = tracking::Entry::default();

        fn iter_tree(source: &Manifest, tree: &Tree, parent: &mut tracking::Entry) {
            for entry in tree.entries.iter() {
                let mut new_entry = tracking::Entry::default();
                new_entry.kind = entry.kind;
                new_entry.mode = entry.mode;
                new_entry.size = entry.size;
                if let tracking::EntryKind::Tree = entry.kind {
                    iter_tree(
                        source,
                        source.trees.get(&entry.object).unwrap(),
                        &mut new_entry,
                    )
                } else {
                    new_entry.object = entry.object;
                }
                parent.entries.insert(entry.name.clone(), new_entry);
            }
        }

        iter_tree(&self, &self.root(), &mut root);
        tracking::Manifest::new(root)
    }
}

impl Encodable for Manifest {
    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        let delta = self.trees.len() as i64 - self.tree_order.len() as i64;
        if delta > 0 {
            return Err("manifest is internally inconsistent (index < count)".into());
        } else if delta < 0 {
            return Err("manifest is internally inconsistent (index > count)".into());
        }

        encoding::write_digest(&mut writer, &self.root)?;
        encoding::write_uint(&mut writer, self.trees.len() as u64)?;
        for digest in &self.tree_order {
            match self.trees.get(&digest) {
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
    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let mut manifest = Manifest::default();
        manifest.root = encoding::read_digest(&mut reader)?;
        let num_trees = encoding::read_uint(&mut reader)?;
        for _ in 0..num_trees {
            let tree = Tree::decode(reader)?;
            manifest.insert_tree(tree)?;
        }
        Ok(manifest)
    }
}
