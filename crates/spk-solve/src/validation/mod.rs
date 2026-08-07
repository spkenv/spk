// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod impossible_checks;
mod validator;
pub mod validators;

pub use error::{Error, Result};
pub use impossible_checks::{IMPOSSIBLE_CHECKS_TARGET, ImpossibleRequestsChecker};
pub use validator::{GetMergedRequest, ValidatorT, Validators, default_validators};
