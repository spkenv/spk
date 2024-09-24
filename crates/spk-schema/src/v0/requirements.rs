// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};

use crate::RequirementsList;

/// For a list of requirements parsed from the requests file
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct Requirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    // Could separate override options out:
    //
    // // From BuildSpec - Opt has var and pkg items
    // #[serde(default, skip_serializing_if = "Vec::is_empty")]
    // pub options: Vec<Opt>,
    //
    // // From V0::Variant
    // #[serde(flatten)]
    // pub options: OptionMap,
}
