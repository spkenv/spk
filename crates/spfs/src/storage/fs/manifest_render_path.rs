// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;
use std::sync::Arc;

use crate::{Result, graph};

/// A trait for types that can map a manifest to a render path on disk.
pub trait ManifestRenderPath {
    /// Return the path that the manifest would be rendered to.
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf>;
}

impl<T> ManifestRenderPath for &T
where
    T: ManifestRenderPath,
{
    #[inline]
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        T::manifest_render_path(self, manifest)
    }
}

impl<T> ManifestRenderPath for Arc<T>
where
    T: ManifestRenderPath,
{
    #[inline]
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        T::manifest_render_path(self, manifest)
    }
}
