// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::{encoding, graph, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

pub trait ManifestStorage: graph::Database {
    /// Iterate the objects in this storage which are manifests.
    fn iter_manifests<'db>(
        &'db self,
    ) -> Box<dyn Iterator<Item = graph::Result<(encoding::Digest, graph::Manifest)>> + 'db> {
        use graph::Object;
        Box::new(self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Manifest(manifest) => Some(Ok((digest, manifest))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        }))
    }

    /// Return true if the identified manifest exists in this storage.
    fn has_manifest(&self, digest: &encoding::Digest) -> bool {
        self.read_manifest(digest).is_ok()
    }

    /// Return the manifest identified by the given digest.
    fn read_manifest(&self, digest: &encoding::Digest) -> Result<graph::Manifest> {
        use graph::Object;
        match self.read_object(digest) {
            Err(err) => Err(err),
            Ok(Object::Manifest(manifest)) => Ok(manifest),
            Ok(_) => Err(format!("Object is not a manifest: {:?}", digest).into()),
        }
    }
}

impl<T: ManifestStorage> ManifestStorage for &mut T {}

pub trait ManifestViewer {
    /// Returns true if the identified manifest has been rendered already
    fn has_rendered_manifest(&self, digest: &encoding::Digest) -> bool;

    /// Create a rendered view of the given manifest on the local disk.
    ///
    /// Returns the local path to the root of the rendered manifest
    fn render_manifest(&self, manifest: &graph::Manifest) -> Result<std::path::PathBuf>;

    /// Cleanup a previously rendered manifest from the local disk.
    fn remove_rendered_manifest(&self, digest: &encoding::Digest) -> Result<()>;
}
