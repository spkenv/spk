// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spfs_encoding as encoding;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "strategy")]
pub enum MountStrategy {
    #[cfg(target_os = "linux")]
    OverlayFS(super::overlayfs::Config),
}

impl Default for MountStrategy {
    fn default() -> Self {
        Self::OverlayFS(Default::default())
    }
}

#[async_trait::async_trait]
impl super::FileSystem for MountStrategy {
    async fn mount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send,
    {
        match self {
            Self::OverlayFS(fs) => fs.mount(stack, editable).await,
        }
    }

    async fn remount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send,
    {
        match self {
            Self::OverlayFS(fs) => fs.remount(stack, editable).await,
        }
    }

    fn reset<S: AsRef<str>>(&self, paths: &[S]) -> Result<()> {
        match self {
            Self::OverlayFS(fs) => fs.reset(paths),
        }
    }

    fn is_dirty(&self) -> bool {
        match self {
            Self::OverlayFS(fs) => fs.is_dirty(),
        }
    }
}
