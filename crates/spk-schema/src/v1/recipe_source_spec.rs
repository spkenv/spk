// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use super::TestScript;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct RecipeSourceSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collect: Vec<crate::SourceSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,
}

impl RecipeSourceSpec {
    pub fn is_empty(&self) -> bool {
        self.collect.is_empty()
    }
}
