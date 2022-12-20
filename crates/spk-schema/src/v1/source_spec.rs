// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::TestScript;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct SourceSpec {
    #[serde(default = "default_sources", skip_serializing_if = "Vec::is_empty")]
    pub collect: Vec<crate::SourceSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,

    /// reserved to help avoid common mistakes in production
    #[serde(default, deserialize_with = "no_tests_field", skip_serializing)]
    tests: PhantomData<()>,
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
            tests: PhantomData,
        }
    }
}

fn default_sources() -> Vec<crate::SourceSpec> {
    vec![crate::SourceSpec::Local(Default::default())]
}

pub(super) fn no_tests_field<'de, D>(_deserializer: D) -> Result<PhantomData<()>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Err(serde::de::Error::custom(
        "no field 'tests', but there is a 'test' field",
    ))
}
