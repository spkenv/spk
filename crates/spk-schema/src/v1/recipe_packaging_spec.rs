// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct RecipePackagingSpec {}
