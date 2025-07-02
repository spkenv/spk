// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::Path;

use super::{RenderType, Renderer};
use crate::prelude::*;
use crate::storage::LocalRepository;
use crate::storage::fs::RenderReporter;
use crate::{Result, graph};

impl<'repo, Repo, Reporter> Renderer<'repo, Repo, Reporter>
where
    Repo: Repository + LocalRepository,
    Reporter: RenderReporter,
{
    /// Recreate the full structure of a stored manifest on disk.
    pub async fn render_manifest_into_dir<P>(
        &self,
        _manifest: &graph::Manifest,
        _target_dir: P,
        _render_type: RenderType,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        unimplemented!()
    }
}
