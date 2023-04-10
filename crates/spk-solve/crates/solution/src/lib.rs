// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod solution;

pub use error::{Error, Result};
pub use solution::{
    get_spfs_layers_to_packages,
    LayerPackageAndComponents,
    PackageSource,
    Solution,
    SolvedRequest,
};
