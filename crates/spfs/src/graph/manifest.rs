// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashMap};
use std::io::BufRead;

use spfs_proto::ManifestArgs;

use super::object::HeaderBuilder;
use super::{Entry, ObjectKind, Tree};
use crate::prelude::*;
use crate::{encoding, tracking, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

/// A manifest holds the state of a filesystem tree.
pub type Manifest = super::object::FlatObject<spfs_proto::Manifest<'static>>;

/// A mapping of digest to tree in a manifest.
pub type ManifestTreeCache<'a> = HashMap<encoding::Digest, Tree<'a>>;

impl Default for Manifest {
    fn default() -> Self {
        Self::builder().build(&crate::tracking::Entry::<()>::empty_dir_with_open_perms())
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

impl Manifest {
    #[inline]
    pub fn builder() -> ManifestBuilder {
        ManifestBuilder::default()
    }

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

    /// Return the trees of this Manifest mapped by digest.
    ///
    /// It is expensive to find a tree in a manifest by digest. If multiple
    /// trees need to be accessed by digest, it is faster to use this method
    /// instead of [`Manifest::get_tree`].
    pub fn get_tree_cache(&self) -> ManifestTreeCache {
        let mut tree_cache = HashMap::new();
        for tree in self.iter_trees() {
            let Ok(digest) = tree.digest() else {
                tracing::warn!("Undigestible tree found in manifest");
                continue;
            };
            tree_cache.insert(digest, tree);
        }
        tree_cache
    }

    /// Return the tree in this manifest with the given digest.
    ///
    /// # Warning
    ///
    /// This can be very slow to call repeatedly on the same manifest. See
    /// [`Manifest::get_tree_cache`].
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
        let tree_cache = self.get_tree_cache();

        let mut root = tracking::Entry::empty_dir_with_open_perms();

        fn iter_tree(
            tree_cache: &ManifestTreeCache,
            tree: &Tree<'_>,
            parent: &mut tracking::Entry,
        ) {
            for entry in tree.entries() {
                let mut new_entry = tracking::Entry {
                    kind: entry.kind(),
                    mode: entry.mode(),
                    entries: Default::default(),
                    object: *entry.object(),
                    user_data: (),
                    legacy_size: entry.size_for_legacy_encode(),
                };
                if entry.kind().is_tree() {
                    new_entry.object = encoding::NULL_DIGEST.into();
                    iter_tree(
                        tree_cache,
                        tree_cache
                            .get(entry.object())
                            .expect("manifest is internally inconsistent (missing child tree)"),
                        &mut new_entry,
                    )
                }
                parent.entries.insert(entry.name().to_owned(), new_entry);
            }
        }

        iter_tree(&tree_cache, &self.root(), &mut root);
        let mut manifest = tracking::Manifest::new(root);
        // ensure that the manifest will round-trip in the case of it
        // being converted back into this type
        manifest.set_header(self.header().to_owned());
        manifest
    }

    pub(super) fn digest_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        self.root().digest_encode(&mut writer)?;
        // this method encodes the root tree first, and does not
        // include it in the count of remaining trees since at least
        // one root is always required
        encoding::write_uint64(&mut writer, self.proto().trees().len() as u64 - 1)?;
        // skip the root tree when saving the rest
        for tree in self.iter_trees().skip(1) {
            tree.digest_encode(writer)?;
        }
        Ok(())
    }

    pub(super) fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        self.root().legacy_encode(&mut writer)?;
        // this method encodes the root tree first, and does not
        // include it in the count of remaining trees since at least
        // one root is always required
        encoding::write_uint64(&mut writer, self.proto().trees().len() as u64 - 1)?;
        // skip the root tree when saving the rest
        for tree in self.iter_trees().skip(1) {
            tree.legacy_encode(writer)?;
        }
        Ok(())
    }
}

pub struct ManifestBuilder {
    header: super::object::HeaderBuilder,
}

impl Default for ManifestBuilder {
    fn default() -> Self {
        Self {
            header: super::object::HeaderBuilder::new(ObjectKind::Manifest),
        }
    }
}

impl ManifestBuilder {
    pub fn with_header<F>(mut self, mut header: F) -> Self
    where
        F: FnMut(HeaderBuilder) -> HeaderBuilder,
    {
        self.header = header(self.header).with_object_kind(ObjectKind::Manifest);
        self
    }

    /// Build a manifest that contains `source` as the root
    /// entry. If `source` is not a tree, an empty manifest is
    /// returned.
    pub fn build<T>(&self, source: &tracking::Entry<T>) -> Manifest
    where
        T: std::cmp::Eq + std::cmp::PartialEq,
    {
        super::BUILDER.with_borrow_mut(|builder| {
            let trees = Self::build_from_entry(builder, source);
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
                Manifest::new_with_header(self.header.build(), builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }

    /// Read a data encoded using the legacy format, and
    /// use the data to fill and complete this builder
    pub fn legacy_decode(self, mut reader: &mut impl BufRead) -> Result<Manifest> {
        super::BUILDER.with_borrow_mut(|builder| {
            // historically, the root tree was stored first an not included in the count
            // since it is an error to not have at least one root tree
            let root = Tree::legacy_decode(builder, &mut reader)?;
            let num_trees = encoding::read_uint64(&mut reader)?;
            let mut trees = Vec::with_capacity(num_trees as usize + 1);
            trees.push(root);
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
                Manifest::new_with_header(self.header.build(), builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            Ok(obj)
        })
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
                    let sub = Self::build_from_entry(builder, node.entry);
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
                        node.entry.size_for_legacy_encode(),
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
}
