// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use encoding::Digestible;
use futures::stream::Stream;
use tokio_stream::StreamExt;

use crate::graph::{self, DigestFromEncode, DigestFromKindAndEncode, Stack};
use crate::{encoding, Result};

#[derive(Debug)]
pub enum PlatformVersion {
    V1(graph::Platform<graph::DigestFromEncode>),
    V2(graph::Platform<graph::DigestFromKindAndEncode>),
}

impl PlatformVersion {
    pub fn digest(&self) -> Result<encoding::Digest> {
        match self {
            PlatformVersion::V1(o) => o.digest(),
            PlatformVersion::V2(o) => o.digest(),
        }
    }

    pub fn stack(&self) -> &Stack {
        match self {
            PlatformVersion::V1(o) => &o.stack,
            PlatformVersion::V2(o) => &o.stack,
        }
    }
}

impl From<graph::Platform<graph::DigestFromEncode>> for PlatformVersion {
    fn from(value: graph::Platform<graph::DigestFromEncode>) -> Self {
        PlatformVersion::V1(value)
    }
}

impl From<graph::Platform<graph::DigestFromKindAndEncode>> for PlatformVersion {
    fn from(value: graph::Platform<graph::DigestFromKindAndEncode>) -> Self {
        PlatformVersion::V2(value)
    }
}

pub type PlatformStreamItem = Result<(encoding::Digest, PlatformVersion)>;

#[async_trait::async_trait]
pub trait PlatformStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are platforms.
    fn iter_platforms<'db>(&'db self) -> Pin<Box<dyn Stream<Item = PlatformStreamItem> + 'db>> {
        use graph::Object;
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::PlatformV1(platform) => Some(Ok((digest, PlatformVersion::V1(platform)))),
                Object::PlatformV2(platform) => Some(Ok((digest, PlatformVersion::V2(platform)))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the platform identified by the given digest.
    async fn read_platform(&self, digest: encoding::Digest) -> Result<PlatformVersion> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::PlatformV1(platform)) => Ok(PlatformVersion::V1(platform)),
            Ok(Object::PlatformV2(platform)) => Ok(PlatformVersion::V2(platform)),
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
            graph::Object::PlatformV1(platform) => Ok(platform),
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
            graph::Object::PlatformV2(platform) => Ok(platform),
            _ => unreachable!(),
        }
    }
}

impl<T: PlatformStorage> PlatformStorage for &T {}
