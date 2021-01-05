use std::collections::HashSet;

use super::{PayloadStorage, TagStorage};
use crate::{encoding, graph, tracking, Result};
use encoding::Encodable;
use graph::{Blob, Manifest};

#[cfg(test)]
#[path = "./repository_test.rs"]
mod repository_test;

#[derive(Debug, Eq, PartialEq, Hash)]
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
pub trait Repository: TagStorage + PayloadStorage + graph::Database {
    /// Attempt to open this repository at the given url
    //fn open(address: url::Url) -> Result<Self>;

    /// Return the address of this repository.
    fn address(&self) -> url::Url;

    /// Return true if this repository contains the given reference.
    fn has_ref(&self, reference: &str) -> bool {
        self.read_ref(reference).is_ok()
    }

    /// Read an object of unknown type by tag or digest.
    fn read_ref(&self, reference: &str) -> Result<graph::Object> {
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
        Ok(aliases)
    }

    /// Commit the data from 'reader' as a blob in this repository
    fn commit_blob(&mut self, reader: &mut impl std::io::Read) -> Result<encoding::Digest> {
        let (digest, size) = self.write_payload(reader)?;
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
