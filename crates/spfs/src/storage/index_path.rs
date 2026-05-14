// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use crate::Result;

/// The index location path of a repository.
#[async_trait::async_trait]
pub trait IndexPath {
    /// Get the index location path of this repository, will create it
    /// if it does not exist.
    async fn index_path(&self) -> Result<PathBuf>;
}
