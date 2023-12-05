// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::stream::Stream;
use once_cell::sync::Lazy;
use tokio_stream::StreamExt;

use crate::graph::{self, DigestFromEncode, DigestFromKindAndEncode, PlatformHandle};
use crate::{encoding, Result};

pub type PlatformStreamItem = Result<(encoding::Digest, PlatformHandle)>;

static STORAGE_GENERATION: Lazy<u64> = Lazy::new(|| {
    crate::get_config()
        .map(|config| config.storage.generation)
        .unwrap_or(0)
});

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
            Ok(Object::Platform(platform))
                if *STORAGE_GENERATION > 0 || matches!(platform, PlatformHandle::V1(_)) =>
            {
                Ok(platform)
            }
            Ok(Object::Platform(_)) => Err(format!(
                "Platform object version is not allowed by the current storage generation configuration: {digest:?}"
            )
            .into()),
            Ok(_) => Err(format!("Object is not a platform: {digest:?}").into()),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform_impl<P>(&self, layers: graph::Stack) -> Result<PlatformHandle>
    where
        graph::Platform<P>: Into<graph::Object>,
        Self: Sized,
    {
        let platform = graph::Platform::<P>::new(layers);
        let storable: graph::Object = platform.into();
        self.write_object(&storable).await?;
        match storable {
            graph::Object::Platform(platform) => Ok(platform),
            _ => unreachable!(),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    async fn create_platform(&self, layers: graph::Stack) -> Result<PlatformHandle>
    where
        Self: Sized,
    {
        match *STORAGE_GENERATION {
            0 => self.create_platform_impl::<DigestFromEncode>(layers).await,
            _ => {
                self.create_platform_impl::<DigestFromKindAndEncode>(layers)
                    .await
            }
        }
    }
}

impl<T: PlatformStorage> PlatformStorage for &T {}
