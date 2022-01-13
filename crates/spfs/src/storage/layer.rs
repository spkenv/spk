// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use crate::{encoding, graph, Result};
use encoding::Encodable;

#[async_trait::async_trait]
pub trait LayerStorage: graph::Database {
    /// Iterate the objects in this storage which are layers.
    fn iter_layers<'db>(
        &'db self,
    ) -> Pin<Box<dyn Stream<Item = Result<(encoding::Digest, graph::Layer)>> + 'db>> {
        use graph::Object;
        let iter = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Layer(layer) => Some(Ok((digest, layer))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(futures::stream::iter(iter))
    }

    /// Return true if the identified layer exists in this storage.
    fn has_layer(&self, digest: &encoding::Digest) -> bool {
        self.read_layer(digest).is_ok()
    }

    /// Return the layer identified by the given digest.
    fn read_layer(&self, digest: &encoding::Digest) -> Result<graph::Layer> {
        use graph::Object;
        match self.read_object(digest) {
            Err(err) => Err(err),
            Ok(Object::Layer(layer)) => Ok(layer),
            Ok(_) => Err(format!("Object is not a layer: {:?}", digest).into()),
        }
    }

    /// Create and storage a new layer for the given layer.
    fn create_layer(&mut self, manifest: &graph::Manifest) -> Result<graph::Layer> {
        let layer = graph::Layer::new(manifest.digest()?);
        let storable = graph::Object::Layer(layer);
        self.write_object(&storable)?;
        if let graph::Object::Layer(layer) = storable {
            Ok(layer)
        } else {
            panic!("this is impossible!");
        }
    }
}

impl<T: LayerStorage> LayerStorage for &mut T {}
