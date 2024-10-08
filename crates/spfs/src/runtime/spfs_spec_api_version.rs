// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};

/// Supported spfs spec api versions
#[derive(Debug, Deserialize, Serialize, Copy, Clone, Eq, PartialEq, strum::Display)]
pub enum SpfsSpecApiVersion {
    #[serde(rename = "v0/layer", alias = "v0/livelayer")]
    V0Layer,
    #[serde(rename = "v0/layerlist")]
    V0EnvLayerList,
}

impl Default for SpfsSpecApiVersion {
    fn default() -> Self {
        Self::V0Layer
    }
}
