// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use encoding::Encodable;
use futures::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, Result};

pub type LayerStreamItem = Result<(encoding::Digest, graph::Layer)>;

#[async_trait::async_trait]
pub trait LayerStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are layers.
    fn iter_layers<'db>(&'db self) -> Pin<Box<dyn Stream<Item = LayerStreamItem> + 'db>> {
        use graph::Object;
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Layer(layer) => Some(Ok((digest, layer))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the layer identified by the given digest.
    async fn read_layer(&self, digest: encoding::Digest) -> Result<graph::Layer> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::Layer(layer)) => Ok(layer),
            Ok(_) => Err(format!("Object is not a layer: {digest:?}").into()),
        }
    }

    /// Create and storage a new layer for the given layer.
    async fn create_layer(&self, manifest: &graph::Manifest) -> Result<graph::Layer> {
        let layer = graph::Layer::new(manifest.digest()?);
        let storable = graph::Object::Layer(layer);
        self.write_object(&storable).await?;
        if let graph::Object::Layer(layer) = storable {
            Ok(layer)
        } else {
            panic!("this is impossible!");
        }
    }
}

impl<T: LayerStorage> LayerStorage for &T {}
