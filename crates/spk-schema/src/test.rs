// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::{PinnedRequest, RequestWithOptions};
use spk_schema_foundation::option_map::OptionMap;

const BUILD_NAME: &str = "build";
const INSTALL_NAME: &str = "install";
const SOURCES_NAME: &str = "sources";
const TEST_STAGES: &[&str] = &[BUILD_NAME, INSTALL_NAME, SOURCES_NAME];

/// Test is an executable script that runs in a specific
/// spk environment and validates some aspect of a package
pub trait Test {
    fn script(&self) -> String;

    fn additional_requirements(&self) -> Vec<PinnedRequest> {
        Vec::new()
    }

    /// Calculate the additional requirements with options.
    ///
    /// This requires the options of the variant being tested.
    fn additional_requirements_with_options(&self, options: &OptionMap) -> Vec<RequestWithOptions>;
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, strum::EnumIter)]
pub enum TestStage {
    Sources,
    Build,
    Install,
}

impl std::fmt::Display for TestStage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(
            // Note that we need `TestStage::to_string` to produce
            // these exact values in order to match correctly with
            // the spelling in the package yaml.
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
        struct TestStageVisitor;

        impl serde::de::Visitor<'_> for TestStageVisitor {
            type Value = TestStage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a test stage, one of: ")?;
                formatter.debug_list().entries(TEST_STAGES).finish()
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<TestStage, E>
            where
                E: serde::de::Error,
            {
                TestStage::from_str(value)
                    .map_err(|_| serde::de::Error::unknown_variant(value, TEST_STAGES))
            }
        }
        deserializer.deserialize_str(TestStageVisitor)
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
                "Invalid test stage '{other}', must be one of: {TEST_STAGES:?}",
            ))),
        }
    }
}
