// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{OptionMap, Request};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestStage {
    Sources,
    Build,
    Install,
}

impl std::fmt::Display for TestStage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Serialize for TestStage {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            TestStage::Sources => "sources".serialize(serializer),
            TestStage::Build => "build".serialize(serializer),
            TestStage::Install => "install".serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TestStage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "sources" => Ok(Self::Sources),
            "build" => Ok(Self::Build),
            "install" => Ok(Self::Install),
            other => Err(serde::de::Error::custom(format!(
                "Invalid test stage '{}', must be one of: source, build, install",
                other
            ))),
        }
    }
}

/// A set of structured inputs used to build a package.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSpec {
    #[pyo3(get, set)]
    stage: TestStage,
    #[pyo3(get, set)]
    script: String,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    selectors: Vec<OptionMap>,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requirements: Vec<Request>,
}
