// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_ident::Request;

use super::{ScriptBlock, WhenBlock};

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct TestScript {
    pub script: ScriptBlock,
    #[serde(default, skip_serializing_if = "WhenBlock::is_always")]
    pub when: WhenBlock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<Request>,
}
