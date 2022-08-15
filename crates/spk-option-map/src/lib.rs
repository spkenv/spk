// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod format;
mod option_map;

pub use error::{Error, Result};
pub use option_map::{host_options, string_from_scalar, OptionMap, DIGEST_SIZE};
