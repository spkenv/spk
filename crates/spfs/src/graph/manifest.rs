// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::io::BufRead;

use spfs_proto::ManifestArgs;

use super::{Entry, Tree};
use crate::prelude::*;
use crate::{encoding, tracking, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

/// A manifest holds the state of a filesystem tree.
pub type Manifest = super::object::FlatObject<spfs_proto::Manifest<'static>>;

impl Default for Manifest {
    fn default() -> Self {
        Self::from(&crate::tracking::Entry::<()>::empty_dir_with_open_perms())
    }
}

impl std::fmt::Debug for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manifest")
            .field("root", &self.root())
            .field("trees", &self.trees().collect::<Vec<_>>())
            .finish()
    }
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
        super::BUILDER.with_borrow_mut(|builder| {
            let trees = build_from_entry(builder, source);
            let trees = builder.create_vector(&trees);
            let manifest =
                spfs_proto::Manifest::create(builder, &ManifestArgs { trees: Some(trees) });
            let any = spfs_proto::AnyObject::create(
                builder,
                &spfs_proto::AnyObjectArgs {
                    object_type: spfs_proto::Object::Manifest,
                    object: Some(manifest.as_union_value()),
                },
            );
            builder.finish_minimal(any);
            let offset = unsafe {
                // Safety: we have just created this buffer
                // so already know the root type with certainty
                flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                    .object_as_manifest()
                    .unwrap()
                    ._tab
                    .loc()
            };
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained layer
                // which is what we've done
                Self::new_with_default_header(builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }
}

impl Manifest {
    /// Return the root tree object of this manifest.
    pub fn root(&self) -> Tree<'_> {
        self.proto()
            .trees()
            .iter()
            .next()
            .map(Tree::from)
            .expect("should always have at least one tree")
    }

    /// Iterate all of the trees in this manifest (excluding the root).
    pub fn trees(&self) -> impl Iterator<Item = Tree<'_>> {
        self.proto().trees().iter().skip(1).map(Tree::from)
    }

    /// Iterate all of the trees in this manifest starting with the root.
    pub fn iter_trees(&self) -> impl Iterator<Item = Tree<'_>> {
        std::iter::once(self.root()).chain(self.trees())
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        let mut children = BTreeSet::new();
        for tree in self.iter_trees() {
            for entry in tree.entries() {
                if entry.kind().is_blob() {
                    children.insert(*entry.object());
                }
            }
        }
        children.into_iter().collect()
    }

    pub fn get_tree(&self, digest: &encoding::Digest) -> Option<Tree<'_>> {
        self.iter_trees()
            .find(|t| t.digest().ok().as_ref() == Some(digest))
    }

    /// Iterate all of the entries in this manifest.
    pub fn iter_entries(&self) -> impl Iterator<Item = super::Entry<'_>> {
        self.iter_trees().flat_map(Tree::into_entries)
    }

    /// Convert this manifest into a more workable form for editing.
    pub fn to_tracking_manifest(&self) -> tracking::Manifest {
        let mut root = tracking::Entry::empty_dir_with_open_perms();

        fn iter_tree(source: &Manifest, tree: Tree<'_>, parent: &mut tracking::Entry) {
            for entry in tree.entries() {
                let mut new_entry = tracking::Entry {
                    kind: entry.kind(),
                    mode: entry.mode(),
                    size: entry.size(),
                    entries: Default::default(),
                    object: *entry.object(),
                    user_data: (),
                };
                if entry.kind().is_tree() {
                    new_entry.object = encoding::NULL_DIGEST.into();
                    iter_tree(
                        source,
                        source
                            .get_tree(entry.object())
                            .expect("manifest is internally inconsistent (missing child tree)"),
                        &mut new_entry,
                    )
                }
                parent.entries.insert(entry.name().to_owned(), new_entry);
            }
        }

        iter_tree(self, self.root(), &mut root);
        tracking::Manifest::new(root)
    }

    pub(super) fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        self.root().legacy_encode(&mut writer)?;
        // this method encodes the root tree twice. This is not
        // very efficient but maintains the original format
        encoding::write_uint64(&mut writer, self.proto().trees().len() as u64)?;
        for tree in self.iter_trees() {
            tree.legacy_encode(writer)?;
        }
        Ok(())
    }

    pub(super) fn legacy_decode(mut reader: &mut impl BufRead) -> Result<Self> {
        super::BUILDER.with_borrow_mut(|builder| {
            // historically, the root tree was stored twice and then deduplicated
            // when loading. In the new flatbuffer format we can simply ignore the
            // first instance and load the others
            let _root = Tree::legacy_decode(builder, &mut reader)?;
            let num_trees = encoding::read_uint64(&mut reader)?;
            let mut trees = Vec::with_capacity(num_trees as usize);
            for _ in 0..num_trees {
                let tree = Tree::legacy_decode(builder, reader)?;
                trees.push(tree);
            }
            let trees = builder.create_vector(&trees);
            let manifest =
                spfs_proto::Manifest::create(builder, &ManifestArgs { trees: Some(trees) });
            let any = spfs_proto::AnyObject::create(
                builder,
                &spfs_proto::AnyObjectArgs {
                    object_type: spfs_proto::Object::Manifest,
                    object: Some(manifest.as_union_value()),
                },
            );
            builder.finish_minimal(any);
            let offset = unsafe {
                // Safety: we have just created this buffer
                // so already know the root type with certainty
                flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                    .object_as_manifest()
                    .unwrap()
                    ._tab
                    .loc()
            };
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained layer
                // which is what we've done
                Self::new_with_default_header(builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            Ok(obj)
        })
    }
}

fn build_from_entry<'buf, T>(
    builder: &mut flatbuffers::FlatBufferBuilder<'buf>,
    source: &tracking::Entry<T>,
) -> Vec<flatbuffers::WIPOffset<spfs_proto::Tree<'buf>>>
where
    T: std::cmp::Eq + std::cmp::PartialEq,
{
    use flatbuffers::Follow;

    let mut entries: Vec<_> = source.iter_entries().collect();
    let mut roots = Vec::with_capacity(entries.len());
    let mut sub_manifests = Vec::new();
    entries.sort_unstable();

    for node in entries {
        let converted = match node.entry.kind {
            tracking::EntryKind::Tree => {
                let sub = build_from_entry(builder, node.entry);
                let first_offset = sub.first().expect("should always have a root entry");
                let wip_data = builder.unfinished_data();
                // WIPOffset is relative to the end of the buffer
                let loc = wip_data.len() - first_offset.value() as usize;
                let sub_root = unsafe {
                    // Safety: follow requires the offset to be valid
                    // and we trust the one that was just created
                    spfs_proto::Tree::follow(wip_data, loc)
                };
                let sub_root_digest = Tree(sub_root)
                    .digest()
                    .expect("entry should have a valid digest");
                sub_manifests.push(sub);
                Entry::build(
                    builder,
                    node.path.as_str(),
                    node.entry.kind,
                    node.entry.mode,
                    node.entry.size,
                    &sub_root_digest,
                )
            }
            _ => Entry::from(builder, node.path.as_str(), node.entry),
        };
        roots.push(converted);
    }
    let root_entries = builder.create_vector(&roots);
    let root = spfs_proto::Tree::create(
        builder,
        &spfs_proto::TreeArgs {
            entries: Some(root_entries),
        },
    );
    let mut seen_trees = std::collections::HashSet::new();
    std::iter::once(vec![root])
        .chain(sub_manifests)
        .flatten()
        .filter(|t| {
            let wip_data = builder.unfinished_data();
            // WIPOffset is relative to the end of the buffer
            let loc = wip_data.len() - t.value() as usize;
            let t = unsafe {
                // Safety: follow requires the offset to be valid
                // and we trust the one that was just created
                spfs_proto::Tree::follow(wip_data, loc)
            };
            seen_trees.insert(Tree(t).digest().expect("tree should have a valid digest"))
        })
        .collect()
}
