// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs_encoding::{Digest, Digestible, Encodable};
use strum::Display;

use super::{DigestFromEncode, DigestFromKindAndEncode, Kind, Object, ObjectKind, Platform, Stack};
use crate::{Error, Result};

#[derive(Debug, Display, Eq, PartialEq, Clone)]
pub enum PlatformHandle {
    V1(Platform<DigestFromEncode>),
    V2(Platform<DigestFromKindAndEncode>),
}

impl PlatformHandle {
    /// Return the digests of objects that this manifest refers to.
    #[inline]
    pub fn child_objects(&self) -> Vec<Digest> {
        match self {
            Self::V1(platform) => platform.child_objects(),
            Self::V2(platform) => platform.child_objects(),
        }
    }

    /// Return the stack of digests that this manifest refers to.
    #[inline]
    pub fn stack(&self) -> &Stack {
        match self {
            Self::V1(platform) => &platform.stack,
            Self::V2(platform) => &platform.stack,
        }
    }
}

impl Digestible for PlatformHandle {
    type Error = Error;

    #[inline]
    fn digest(&self) -> Result<Digest> {
        match self {
            Self::V1(platform) => platform.digest(),
            Self::V2(platform) => platform.digest(),
        }
    }
}

impl Encodable for PlatformHandle {
    type Error = Error;

    #[inline]
    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        match self {
            Self::V1(platform) => platform.encode(&mut writer),
            Self::V2(platform) => platform.encode(&mut writer),
        }
    }
}

impl Kind for PlatformHandle {
    #[inline]
    fn kind(&self) -> ObjectKind {
        match self {
            Self::V1(_) => ObjectKind::PlatformV1,
            Self::V2(_) => ObjectKind::PlatformV2,
        }
    }
}

impl From<Platform<DigestFromEncode>> for PlatformHandle {
    #[inline]
    fn from(platform: Platform<DigestFromEncode>) -> Self {
        Self::V1(platform)
    }
}

impl From<Platform<DigestFromKindAndEncode>> for PlatformHandle {
    #[inline]
    fn from(platform: Platform<DigestFromKindAndEncode>) -> Self {
        Self::V2(platform)
    }
}

impl From<PlatformHandle> for Object {
    #[inline]
    fn from(platform: PlatformHandle) -> Self {
        Self::Platform(platform)
    }
}
