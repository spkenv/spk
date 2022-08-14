// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use crate::{
    api,
    io::{self, Format},
    prelude::*,
    storage::{self, CachePolicy},
    with_cache_policy, Error, Result,
};

#[cfg(test)]
#[path = "./publish_test.rs"]
mod publish_test;

/// Manages the publishing of packages from one repo to another.
///
/// Usually, the publish process moves packages from the local
/// repo to a shared one, but this is not strictly required.
/// The publisher can be customized after creation before calling
/// the publish method to execute.
pub struct Publisher {
    from: Arc<storage::RepositoryHandle>,
    to: Arc<storage::RepositoryHandle>,
    skip_source_packages: bool,
    force: bool,
}

impl Publisher {
    /// Create a new publisher that moves packages from 'source' to 'destination'.
    ///
    /// The publisher can be further configured before calling [`Publisher::publish`]
    /// to run the operation.
    pub fn new(
        source: Arc<storage::RepositoryHandle>,
        destination: Arc<storage::RepositoryHandle>,
    ) -> Self {
        Self {
            from: source,
            to: destination,
            skip_source_packages: false,
            force: false,
        }
    }

    /// Change the source repository to publish packages from.
    pub fn with_source(mut self, repo: Arc<storage::RepositoryHandle>) -> Self {
        self.from = repo;
        self
    }

    /// Change the destination repository to publish packages into.
    pub fn with_target(mut self, repo: Arc<storage::RepositoryHandle>) -> Self {
        self.to = repo;
        self
    }

    /// Do not publish source packages, even if they exist for the version being published.
    pub fn skip_source_packages(mut self, skip_source_packages: bool) -> Self {
        self.skip_source_packages = skip_source_packages;
        self
    }

    /// Forcefully publishing a package will overwrite an existing publish if it exists.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Publish the identified package as configured.
    pub async fn publish(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        let recipe_ident = pkg.with_build(None);
        tracing::info!("loading recipe: {}", recipe_ident.format_ident());
        match with_cache_policy!(self.from, CachePolicy::BypassCache, {
            self.from.read_recipe(&recipe_ident).await
        }) {
            Err(err @ Error::PackageNotFoundError(_)) if self.force => {
                return Err(
                    format!("Can't force publish; missing package spec locally: {err}").into(),
                );
            }
            Err(Error::PackageNotFoundError(_)) => {
                // If it was not found locally, allow the publish to proceed;
                // if it is also missing on the remote, that will be caught
                // and the publish will be rejected by the storage.
            }
            Err(err) => return Err(err),
            Ok(recipe) => {
                tracing::info!("publishing recipe: {}", recipe.to_ident().format_ident());
                if self.force {
                    self.to.force_publish_recipe(&recipe).await?;
                } else {
                    match self.to.publish_recipe(&recipe).await {
                        Ok(_) | Err(Error::VersionExistsError(_)) => {
                            // It's cool if the version already exists
                        }
                        Err(err) => {
                            return Err(format!(
                                "Failed to publish recipe {}: {err}",
                                recipe.to_ident()
                            )
                            .into())
                        }
                    }
                }
            }
        }

        let builds = if pkg.build().is_none() {
            with_cache_policy!(self.from, CachePolicy::BypassCache, {
                self.from.list_package_builds(pkg)
            })
            .await?
        } else {
            vec![pkg.to_owned()]
        };

        for build in builds.iter() {
            use crate::storage::RepositoryHandle::SPFS;

            if build.is_source() && self.skip_source_packages {
                tracing::info!("skipping source package: {}", build.format_ident());
                continue;
            }

            tracing::debug!("   loading package: {}", build.format_ident());
            let spec = self.from.read_package(build).await?;
            let components = self.from.read_components(build).await?;
            tracing::info!("publishing package: {}", spec.ident().format_ident());
            let env_spec = components.values().cloned().collect();
            match (&*self.from, &*self.to) {
                (SPFS(src), SPFS(dest)) => {
                    tracing::debug!(
                        " syncing components: {}",
                        io::format_components(components.keys())
                    );
                    let syncer = spfs::Syncer::new(src, dest)
                        .with_reporter(spfs::sync::ConsoleSyncReporter::default());
                    syncer.sync_env(env_spec).await?;
                }
                _ => {
                    return Err(Error::String(
                        "Source and destination must both be spfs repositories".into(),
                    ))
                }
            }
            self.to.publish_package(&spec, &components).await?;
        }

        Ok(builds)
    }
}
