// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::stream::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

pub type ManifestStreamItem = Result<(encoding::Digest, graph::Manifest)>;

#[async_trait::async_trait]
pub trait ManifestStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are manifests.
    fn iter_manifests<'db>(&'db self) -> Pin<Box<dyn Stream<Item = ManifestStreamItem> + 'db>> {
        use graph::Object;
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Manifest(manifest) => Some(Ok((digest, manifest))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return true if the identified manifest exists in this storage.
    async fn has_manifest(&self, digest: encoding::Digest) -> bool {
        self.read_manifest(digest).await.is_ok()
    }

    /// Return the manifest identified by the given digest.
    async fn read_manifest(&self, digest: encoding::Digest) -> Result<graph::Manifest> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::Manifest(manifest)) => Ok(manifest),
            Ok(_) => Err(format!("Object is not a manifest: {:?}", digest).into()),
        }
    }
}

impl<T: ManifestStorage> ManifestStorage for &mut T {}

#[async_trait::async_trait]
pub trait ManifestViewer: Send + Sync {
    /// Returns true if the identified manifest has been rendered already
    async fn has_rendered_manifest(&self, digest: encoding::Digest) -> bool;

    /// Create a rendered view of the given manifest on the local disk.
    ///
    /// Returns the local path to the root of the rendered manifest
    async fn render_manifest(&self, manifest: &graph::Manifest) -> Result<std::path::PathBuf>;

    /// Cleanup a previously rendered manifest from the local disk.
    async fn remove_rendered_manifest(&self, digest: encoding::Digest) -> Result<()>;
}
