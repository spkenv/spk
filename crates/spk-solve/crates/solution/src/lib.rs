// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod package_solve_data;
mod solution;

pub use error::{Error, Result};
pub use package_solve_data::{PackageSolveData, PackagesToSolveData, SPK_SOLVE_EXTRA_DATA_KEY};
pub use solution::{
    find_highest_package_version,
    get_spfs_layers_to_packages,
    LayerPackageAndComponents,
    PackageSource,
    Solution,
    SolvedRequest,
};
