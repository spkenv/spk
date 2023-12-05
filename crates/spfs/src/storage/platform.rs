// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::stream::Stream;
use tokio_stream::StreamExt;

use crate::graph::{self, DigestFromEncode, DigestFromKindAndEncode, PlatformHandle};
use crate::{encoding, Result};

pub type PlatformStreamItem = Result<(encoding::Digest, PlatformHandle)>;

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

    /// Return the platform identified by the given digest.
    async fn read_platform(&self, digest: encoding::Digest) -> Result<PlatformHandle> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::Platform(platform)) => Ok(platform),
            Ok(_) => Err(format!("Object is not a platform: {digest:?}").into()),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform_v1(
        &self,
        layers: graph::Stack,
    ) -> Result<graph::Platform<DigestFromEncode>> {
        let platform = graph::Platform::<DigestFromEncode>::new(layers);
        let storable: graph::Object = platform.into();
        self.write_object(&storable).await?;
        match storable {
            graph::Object::Platform(PlatformHandle::V1(platform)) => Ok(platform),
            _ => unreachable!(),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform(
        &self,
        layers: graph::Stack,
    ) -> Result<graph::Platform<DigestFromKindAndEncode>> {
        let platform = graph::Platform::<DigestFromKindAndEncode>::new(layers);
        let storable: graph::Object = platform.into();
        self.write_object(&storable).await?;
        match storable {
            graph::Object::Platform(PlatformHandle::V2(platform)) => Ok(platform),
            _ => unreachable!(),
        }
    }
}

impl<T: PlatformStorage> PlatformStorage for &T {}
