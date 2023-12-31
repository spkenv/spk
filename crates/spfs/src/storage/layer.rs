// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use encoding::prelude::*;
use futures::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, tracking, Result};

pub type LayerStreamItem = Result<(encoding::Digest, graph::Layer)>;

#[async_trait::async_trait]
pub trait LayerStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are layers.
    fn iter_layers<'db>(&'db self) -> Pin<Box<dyn Stream<Item = LayerStreamItem> + 'db>> {
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => obj.into_layer().map(|b| Ok((digest, b))),
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the layer identified by the given digest.
    async fn read_layer(&self, digest: encoding::Digest) -> Result<graph::Layer> {
        match self
            .read_object(digest)
            .await
            .map(graph::Object::into_layer)
        {
            Err(err) => Err(err),
            Ok(Some(layer)) => Ok(layer),
            Ok(None) => Err(crate::Error::NotCorrectKind {
                desired: graph::ObjectKind::Layer,
                digest,
            }),
        }
    }

    /// Create and storage a new layer for the given layer.
    async fn create_layer(&self, manifest: &graph::Manifest) -> Result<graph::Layer> {
        let layer = graph::Layer::new(manifest.digest()?);
        self.write_object(&layer).await?;
        Ok(layer)
    }

    /// Create new layer from an arbitrary manifest
    async fn create_layer_from_manifest(
        &self,
        manifest: &tracking::Manifest,
    ) -> Result<graph::Layer> {
        let storable_manifest = manifest.to_graph_manifest();
        self.write_object(&storable_manifest).await?;
        self.create_layer(&storable_manifest).await
    }
}

impl<T: LayerStorage> LayerStorage for &T {}
