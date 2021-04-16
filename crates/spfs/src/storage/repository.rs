// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use super::ManifestViewer;
use crate::{encoding, graph, tracking, Result};
use encoding::Encodable;
use graph::{Blob, Manifest};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum Ref {
    Digest(encoding::Digest),
    TagSpec(tracking::TagSpec),
}

impl std::string::ToString for Ref {
    fn to_string(&self) -> String {
        match self {
            Self::Digest(d) => d.to_string(),
            Self::TagSpec(t) => t.to_string(),
        }
    }
}

/// Represents a storage location for spfs data.
pub trait Repository:
    super::TagStorage
    + super::PayloadStorage
    + super::ManifestStorage
    + super::BlobStorage
    + super::LayerStorage
    + super::PlatformStorage
    + graph::Database
    + graph::DatabaseView
    + std::fmt::Debug
{
    /// Attempt to open this repository at the given url
    //fn open(address: url::Url) -> Result<Self>;

    /// Return the address of this repository.
    fn address(&self) -> url::Url;

    /// If supported, returns the type responsible for locally rendered manifests
    fn renders(&self) -> Result<Box<dyn ManifestViewer>> {
        Err(format!(
            "Repository does not support local renders: {:?}",
            self.address()
        )
        .into())
    }

    /// Return true if this repository contains the given reference.
    fn has_ref(&self, reference: &str) -> bool {
        self.read_ref(reference).is_ok()
    }

    /// Resolve a tag or digest string into it's absolute digest.
    fn resolve_ref(&self, reference: &str) -> Result<encoding::Digest> {
        let reference = reference.as_ref();
        let digest = if let Ok(tag_spec) = tracking::TagSpec::parse(reference) {
            if let Ok(tag) = self.resolve_tag(&tag_spec) {
                tag.target
            } else {
                self.resolve_full_digest(reference)?
            }
        } else {
            self.resolve_full_digest(reference)?
        };

        Ok(digest)
    }

    /// Read an object of unknown type by tag or digest.
    fn read_ref(&self, reference: &str) -> Result<graph::Object> {
        let digest = self.resolve_ref(reference)?;
        Ok(self.read_object(&digest)?)
    }

    /// Return the other identifiers that can be used for 'reference'.
    fn find_aliases(&self, reference: &str) -> Result<HashSet<Ref>> {
        let mut aliases = HashSet::new();
        let digest = self.read_ref(reference)?.digest()?;
        for spec in self.find_tags(&digest) {
            aliases.insert(Ref::TagSpec(spec?));
        }
        if reference != digest.to_string().as_str() {
            aliases.insert(Ref::Digest(digest));
        }
        let mut dupe = None;
        for alias in aliases.iter().collect::<Vec<_>>() {
            if alias.to_string().as_str() == reference {
                dupe = Some(alias.clone());
                break;
            }
        }
        if let Some(r) = dupe {
            aliases.remove(&r);
        }
        Ok(aliases)
    }

    /// Commit the data from 'reader' as a blob in this repository
    fn commit_blob(&mut self, reader: Box<&mut dyn std::io::Read>) -> Result<encoding::Digest> {
        let (digest, size) = self.write_data(reader)?;
        let blob = Blob::new(digest, size);
        self.write_object(&graph::Object::Blob(blob))?;
        Ok(digest)
    }

    /// Commit a local file system directory to this storage.
    ///
    /// This collects all files to store as blobs and maintains a
    /// render of the manifest for use immediately.
    fn commit_dir(&mut self, path: &std::path::Path) -> Result<tracking::Manifest> {
        let path = std::fs::canonicalize(path)?;
        let mut builder = tracking::ManifestBuilder::new(|reader| self.commit_blob(reader));

        tracing::info!("committing files");
        let manifest = builder.compute_manifest(path)?;
        drop(builder);

        tracing::info!("writing manifest");
        let storable = Manifest::from(&manifest);
        self.write_object(&graph::Object::Manifest(storable))?;
        for node in manifest.walk() {
            if !node.entry.kind.is_blob() {
                continue;
            }
            let blob = Blob::new(node.entry.object, node.entry.size);
            self.write_object(&graph::Object::Blob(blob))?;
        }

        Ok(manifest)
    }
}

impl<T: Repository> Repository for &mut T {
    fn address(&self) -> url::Url {
        Repository::address(&**self)
    }
}
