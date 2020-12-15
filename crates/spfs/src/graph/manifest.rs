use std::collections::{BTreeMap, BTreeSet};

use super::{Entry, Tree};
use crate::encoding::Encodable;
use crate::Result;
use crate::{encoding, tracking};

#[derive(Debug, Eq, PartialEq)]
pub struct Manifest {
    root: encoding::Digest,
    trees: BTreeMap<encoding::Digest, Tree>,
}

impl Default for Manifest {
    fn default() -> Self {
        let mut manifest = Manifest {
            root: encoding::NULL_DIGEST.into(),
            trees: Default::default(),
        };
        // add the default empty tree to make this manifest internally coherent
        manifest
            .trees
            .insert(encoding::NULL_DIGEST.into(), Tree::default());
        manifest
    }
}

impl From<tracking::Manifest> for Manifest {
    fn from(source: tracking::Manifest) -> Self {
        Self::from(&source.take_root())
    }
}

impl From<&tracking::Entry> for Manifest {
    fn from(source: &tracking::Entry) -> Self {
        let mut manifest = Self::default();
        let mut root = Tree::default();
        for (name, entry) in source.entries.iter() {
            let converted = match entry.kind {
                tracking::EntryKind::Tree => {
                    let mut sub = Self::from(entry);
                    manifest.trees.append(&mut sub.trees);
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
        manifest.root = root.digest().unwrap();
        manifest.trees.insert(manifest.root, root);
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
    pub fn child_objects<'a>(&'a self) -> Vec<&'a encoding::Digest> {
        let mut children = BTreeSet::new();
        for tree in self.trees.values() {
            for entry in tree.entries.iter() {
                if let tracking::EntryKind::Blob = entry.kind {
                    children.insert(&entry.object);
                }
            }
        }
        return children.into_iter().collect();
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
    pub fn unlock(self) -> tracking::Manifest {
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
        encoding::write_digest(&mut writer, &self.root)?;
        encoding::write_uint(&mut writer, self.trees.len() as u64)?;
        for tree in self.trees.values() {
            tree.encode(writer)?;
        }
        Ok(())
    }

    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let mut manifest = Manifest::default();
        manifest.root = encoding::read_digest(&mut reader)?;
        let num_trees = encoding::read_uint(&mut reader)?;
        for _ in 0..num_trees {
            let tree = Tree::decode(reader)?;
            manifest.trees.insert(tree.digest().unwrap(), tree);
        }
        Ok(manifest)
    }
}
