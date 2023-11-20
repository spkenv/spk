// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod solution;

pub use error::{Error, Result};
pub use solution::{
    find_highest_package_version,
    get_spfs_layers_to_packages,
    LayerPackageAndComponents,
    PackageSource,
    Solution,
    SolvedRequest,
};
