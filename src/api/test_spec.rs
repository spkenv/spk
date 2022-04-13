// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{OptionMap, Request};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TestStage {
    Sources,
    Build,
    Install,
}

const BUILD_NAME: &str = "build";
const INSTALL_NAME: &str = "install";
const SOURCES_NAME: &str = "sources";

impl std::fmt::Display for TestStage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(
            // Note that we need `TestStage::to_string` to produce
            // these exact values in order to match correctly with
            // the spelling on the python side and in the package
            // yaml.
            match self {
                TestStage::Build => BUILD_NAME,
                TestStage::Install => INSTALL_NAME,
                TestStage::Sources => SOURCES_NAME,
            },
        )
    }
}

impl Serialize for TestStage {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TestStage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

impl FromStr for TestStage {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            SOURCES_NAME => Ok(Self::Sources),
            BUILD_NAME => Ok(Self::Build),
            INSTALL_NAME => Ok(Self::Install),
            other => Err(crate::Error::String(format!(
                "Invalid test stage '{}', must be one of: {}, {}, {}",
                other, SOURCES_NAME, BUILD_NAME, INSTALL_NAME,
            ))),
        }
    }
}

/// A set of structured inputs used to build a package.
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSpec {
    #[pyo3(get, set)]
    pub stage: TestStage,
    #[pyo3(get, set)]
    #[serde(deserialize_with = "super::build_spec::deserialize_script")]
    pub script: Vec<String>,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<OptionMap>,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Request>,
}
