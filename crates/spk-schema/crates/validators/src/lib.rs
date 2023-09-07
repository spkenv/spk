// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod validators;

pub use error::{Error, Result};
pub use validators::{
    must_collect_all_files,
    must_install_something,
    must_not_alter_existing_files,
    ValidationErrorFilterResult,
    Validator,
};
