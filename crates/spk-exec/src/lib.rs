// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod exec;

pub use error::{Error, Result};
pub use exec::{
    pull_resolved_runtime_layers,
    resolve_runtime_layers,
    setup_current_runtime,
    setup_runtime,
    solution_to_resolved_runtime_layers,
    ResolvedLayer,
};
