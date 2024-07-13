// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod impossible_checks;
mod validation;
pub mod validators;

pub use error::{Error, Result};
pub use impossible_checks::{ImpossibleRequestsChecker, IMPOSSIBLE_CHECKS_TARGET};
pub use validation::{default_validators, GetMergedRequest, ValidatorT, Validators};
