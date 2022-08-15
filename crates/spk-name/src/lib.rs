// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]

mod error;
mod name;
pub mod parsing;

pub use error::{Error, Result};
pub use name::{
    validate_tag_name, InvalidNameError, OptName, OptNameBuf, PkgName, PkgNameBuf, RepositoryName,
    RepositoryNameBuf,
};
