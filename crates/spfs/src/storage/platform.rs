// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::stream::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, Result};

pub type PlatformStreamItem = Result<(encoding::Digest, graph::Platform)>;

#[async_trait::async_trait]
pub trait PlatformStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are platforms.
    fn iter_platforms<'db>(&'db self) -> Pin<Box<dyn Stream<Item = PlatformStreamItem> + 'db>> {
        use graph::Object;
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Platform(platform) => Some(Ok((digest, platform))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return true if the identified platform exists in this storage.
    async fn has_platform(&self, digest: encoding::Digest) -> bool {
        self.read_platform(digest).await.is_ok()
    }

    /// Return the platform identified by the given digest.
    async fn read_platform(&self, digest: encoding::Digest) -> Result<graph::Platform> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::Platform(platform)) => Ok(platform),
            Ok(_) => Err(format!("Object is not a platform: {:?}", digest).into()),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform(&self, layers: Vec<encoding::Digest>) -> Result<graph::Platform> {
        let platform = graph::Platform::new(layers.into_iter())?;
        let storable = graph::Object::Platform(platform);
        self.write_object(&storable).await?;
        if let graph::Object::Platform(platform) = storable {
            Ok(platform)
        } else {
            panic!("this is impossible!");
        }
    }
}

impl<T: PlatformStorage> PlatformStorage for &T {}
