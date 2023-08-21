// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;

use crate::{graph, Result};

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
