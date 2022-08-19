// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use spk_build::BinaryPackageBuilder;
use spk_foundation::format::{FormatIdent, FormatOptionMap};
use spk_foundation::spec_ops::PackageOps;
use spk_solve::solution::{PackageSource, Solution};
use spk_storage::{self as storage};

use crate::Result;

/// Build any packages in the given solution that need building.
///
/// Returns a new solution of only binary packages.
pub async fn build_required_packages(solution: &Solution) -> Result<Solution> {
    let handle: storage::RepositoryHandle = storage::local_repository().await?.into();
    let local_repo = Arc::new(handle);
    let repos = solution.repositories();
    let options = solution.options();
    let mut compiled_solution = Solution::new(Some(options.clone()));
    for item in solution.items() {
        let recipe = match item.source {
            PackageSource::BuildFromSource { recipe } => recipe,
            source => {
                compiled_solution.add(&item.request, item.spec, source);
                continue;
            }
        };

        tracing::info!(
            "Building: {} for {}",
            item.spec.ident().format_ident(),
            options.format_option_map()
        );
        let (package, components) = BinaryPackageBuilder::from_recipe((*recipe).clone())
            .with_repositories(repos.clone())
            .with_options(options.clone())
            .build_and_publish(&*local_repo)
            .await?;
        let source = PackageSource::Repository {
            repo: local_repo.clone(),
            components,
        };
        compiled_solution.add(&item.request, Arc::new(package), source);
    }
    Ok(compiled_solution)
}
