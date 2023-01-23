// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod impossible_checks;
mod validation;

pub use error::{Error, Result};
pub use impossible_checks::{ImpossibleRequestsChecker, IMPOSSIBLE_CHECKS_TARGET};
pub use validation::{
    default_validators,
    AllValidatableData,
    BinaryOnlyValidator,
    OnlyPackageRequestsData,
    ValidatorT,
    Validators,
};
