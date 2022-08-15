// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod exec;

pub use error::{Error, Result};
pub use exec::{resolve_runtime_layers, setup_current_runtime, setup_runtime};
