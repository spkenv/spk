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
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => obj.into_platform().map(|b| Ok((digest, b))),
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the platform identified by the given digest.
    async fn read_platform(&self, digest: encoding::Digest) -> Result<graph::Platform> {
        match self
            .read_object(digest)
            .await
            .map(graph::Object::into_platform)
        {
            Err(err) => Err(err),
            Ok(Some(platform)) => Ok(platform),
            Ok(None) => Err(crate::Error::NotCorrectKind {
                desired: graph::ObjectKind::Platform,
                digest,
            }),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform(&self, layers: graph::Stack) -> Result<graph::Platform> {
        let platform = graph::Platform::from(layers);
        self.write_object(&platform).await?;
        Ok(platform)
    }
}

impl<T: PlatformStorage> PlatformStorage for &T {}
