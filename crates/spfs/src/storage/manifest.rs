// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::pin::Pin;

use futures::stream::Stream;
use tokio_stream::StreamExt;

use crate::{Result, encoding, graph};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

pub type ManifestStreamItem = Result<(encoding::Digest, graph::Manifest)>;

#[async_trait::async_trait]
pub trait ManifestStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are manifests.
    fn iter_manifests<'db>(&'db self) -> Pin<Box<dyn Stream<Item = ManifestStreamItem> + 'db>> {
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok(graph::DatabaseItem::Object(digest, obj)) => {
                obj.into_manifest().map(|b| Ok((digest, b)))
            }
            Ok(graph::DatabaseItem::Payload(_digest)) => None,
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the manifest identified by the given digest.
    async fn read_manifest(&self, digest: encoding::Digest) -> Result<graph::Manifest> {
        match self
            .read_object(digest)
            .await
            .map(graph::Object::into_manifest)
        {
            Err(err) => Err(err),
            Ok(Some(manifest)) => Ok(manifest),
            Ok(None) => Err(crate::Error::NotCorrectKind {
                desired: graph::ObjectKind::Manifest,
                digest,
            }),
        }
    }
}

/// Blanket implementation.
impl<T> ManifestStorage for T where T: graph::Database + Sync + Send {}
