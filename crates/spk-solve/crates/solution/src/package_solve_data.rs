// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use spk_schema::ident::RequestedBy;
use spk_schema::BuildIdent;

/// Key for extra data stored in spfs runtimes by spk when creating a
/// runtime and read back in by spk commands run inside that spfs/spk
/// environment.
pub const SPK_SOLVE_EXTRA_DATA_KEY: &str = "spk_solve";

/// Current data structure version number for PackageToSolveData
pub const PACKAGE_TO_SOLVE_DATA_VERSION: u32 = 1;

/// Holds the extra solve related data for a package
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PackageSolveData {
    /// What a resolved package was requested by
    pub requested_by: Vec<RequestedBy>,
    /// Name of the repo the resolve package was found in. Optional
    /// because embedded packages will have not have source repos.
    pub source_repo_name: Option<String>,
}

/// The extra solve data for all the resolve packages for saving in
/// the spfs runtime's created by spk after a solver run
#[derive(Default, Serialize, Deserialize)]
pub struct PackagesToSolveData {
    /// For tracking data structure changes
    version: u32,
    /// Resolved package id to solve data mapping
    data: BTreeMap<BuildIdent, PackageSolveData>,
}

impl PackagesToSolveData {
    pub fn get(&self, key: &BuildIdent) -> Option<&PackageSolveData> {
        self.data.get(key)
    }
}

impl From<BTreeMap<BuildIdent, PackageSolveData>> for PackagesToSolveData {
    fn from(data: BTreeMap<BuildIdent, PackageSolveData>) -> Self {
        let version = PACKAGE_TO_SOLVE_DATA_VERSION;
        PackagesToSolveData { version, data }
    }
}
