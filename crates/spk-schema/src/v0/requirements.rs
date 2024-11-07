// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RequirementsList;

/// For a list of requirements parsed from the requests file
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct Requirements {
    /// A list of var or pkg requests
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,

    /// Additional options for templates and solver's initial options
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}
