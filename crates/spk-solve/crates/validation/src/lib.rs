// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod impossible_checks;
mod validation;
pub mod validators;

pub use error::{Error, Result};
pub use impossible_checks::{ImpossibleRequestsChecker, IMPOSSIBLE_CHECKS_TARGET};
pub use validation::{default_validators, GetMergedRequest, ValidatorT, Validators};
