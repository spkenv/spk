// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};

use super::FileMatcher;
use crate::ident_component::Component;

/// Control how files are filtered between components.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ComponentFileMatchMode {
    /// Matching files are always included.
    #[default]
    All,
    /// Matching files are only included if they haven't already been matched
    /// by a previously defined component.
    Remaining,
}

pub trait ComponentOps {
    fn file_match_mode(&self) -> &ComponentFileMatchMode;
    fn files(&self) -> &FileMatcher;
    fn name(&self) -> &Component;
    fn uses(&self) -> &[Component];
}
