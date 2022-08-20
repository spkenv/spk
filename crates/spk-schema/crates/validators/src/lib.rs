// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod validators;

pub use error::{Error, Result};
pub use validators::{
    must_collect_all_files, must_install_something, must_not_alter_existing_files,
};
