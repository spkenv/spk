// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use crate::{api, io, storage, Error, Result};

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
        let builds = if pkg.build.is_none() {
            tracing::info!("loading spec: {}", io::format_ident(pkg));
            match self.from.read_spec(pkg).await {
                Err(Error::PackageNotFoundError(_)) => (),
                Err(err) => return Err(err),
                Ok(spec) => {
                    tracing::info!("publishing spec: {}", io::format_ident(&spec.pkg));
                    if self.force {
                        self.to.force_publish_spec(spec).await?;
                    } else {
                        self.to.publish_spec(spec).await?;
                    }
                }
            }

            self.from.list_package_builds(pkg).await?
        } else {
            vec![pkg.to_owned()]
        };

        for build in builds.iter() {
            use crate::storage::RepositoryHandle::SPFS;

            if build.is_source() && self.skip_source_packages {
                tracing::info!("skipping source package: {}", io::format_ident(build));
                continue;
            }

            tracing::debug!("   loading package: {}", io::format_ident(build));
            let spec = self.from.read_spec(build).await?;
            let components = self.from.get_package(build).await?;
            tracing::info!("publishing package: {}", io::format_ident(&spec.pkg));
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
            self.to.publish_package(spec, components).await?;
        }

        Ok(builds)
    }
}
