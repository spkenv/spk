// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};

/// Supported spfs spec api versions
#[derive(Debug, Deserialize, Serialize, Copy, Clone, Eq, PartialEq, strum::Display)]
pub enum SpecApiVersion {
    #[serde(
        rename = "spfs/v0/livelayer",
        alias = "v0/livelayer",
        alias = "v0/layer"
    )]
    V0Layer,
    #[serde(rename = "spfs/v0/runspec")]
    V0EnvLayerList,
}

impl Default for SpecApiVersion {
    fn default() -> Self {
        Self::V0Layer
    }
}
