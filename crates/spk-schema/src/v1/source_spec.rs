// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use super::TestScript;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct SourceSpec {
    #[serde(default = "default_sources", skip_serializing_if = "Vec::is_empty")]
    pub collect: Vec<crate::SourceSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,
}

impl SourceSpec {
    pub fn is_empty(&self) -> bool {
        self.collect.is_empty()
    }
}

impl Default for SourceSpec {
    fn default() -> Self {
        Self {
            collect: default_sources(),
            test: Vec::new(),
        }
    }
}

fn default_sources() -> Vec<crate::SourceSpec> {
    vec![crate::SourceSpec::Local(Default::default())]
}
