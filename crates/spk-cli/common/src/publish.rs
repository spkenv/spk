// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;
use std::sync::Arc;

use spk_schema::foundation::format::{FormatComponents, FormatIdent};
use spk_schema::foundation::ident_component::ComponentSet;
use spk_schema::ident::AsVersionIdent;
use spk_schema::{AnyIdent, BuildIdent, Package, Recipe, VersionIdent};
use spk_storage as storage;
use storage::{with_cache_policy, CachePolicy};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./publish_test.rs"]
mod publish_test;

/// Contains label=value data for use with publishing
#[derive(Debug, Clone)]
pub struct PublishLabel {
    label: String,
    value: String,
}

impl FromStr for PublishLabel {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some((label, value)) = s.split_once('=') {
            return Ok(Self {
                label: label.to_string(),
                value: value.to_string(),
            });
        }
        Err(Error::String(format!(
            "Invalid: {s} is not in 'label=value' format"
        )))
    }
}

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
    allow_existing_label: Option<PublishLabel>,
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
            allow_existing_label: None,
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

    /// Allow publishing builds when the version already exists and
    /// the existing version recipe contains the given metadata label
    /// and value.
    pub fn allow_existing_with_label(mut self, existing_label: Option<PublishLabel>) -> Self {
        self.allow_existing_label = existing_label;
        self
    }

    /// Forcefully publishing a package will overwrite an existing publish if it exists.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    async fn allow_existing_version(&self, dest_recipe_ident: &VersionIdent) -> Result<bool> {
        if let Some(label) = &self.allow_existing_label {
            let dest_recipe = self.to.read_recipe(dest_recipe_ident).await?;
            let result = dest_recipe
                .metadata()
                .has_label_with_value(&label.label, &label.value);
            if !result {
                tracing::debug!(
                    "Package version does not have '{}={}' in its metadata",
                    label.label,
                    label.value
                );
            }
            Ok(result)
        } else {
            Ok(false)
        }
    }

    /// Publish the identified package as configured.
    pub async fn publish<I>(&self, pkg: I) -> Result<Vec<BuildIdent>>
    where
        I: AsRef<AnyIdent>,
    {
        let pkg = pkg.as_ref();
        let recipe_ident = pkg.as_version_ident();
        tracing::info!("loading recipe: {}", recipe_ident.format_ident());
        match with_cache_policy!(self.from, CachePolicy::BypassCache, {
            self.from.read_recipe(recipe_ident).await
        }) {
            Err(err @ spk_storage::Error::PackageNotFound(_)) if self.force => {
                return Err(
                    format!("Can't force publish; missing package spec locally: {err}").into(),
                );
            }
            Err(spk_storage::Error::PackageNotFound(_)) => {
                // If it was not found locally, allow the publish to proceed;
                // if it is also missing on the remote, that will be caught
                // and the publish will be rejected by the storage.
            }
            Err(err) => return Err(err.into()),
            Ok(recipe) => {
                tracing::info!("publishing recipe: {}", recipe.ident().format_ident());
                if self.force {
                    self.to.force_publish_recipe(&recipe).await?;
                } else {
                    match self.to.publish_recipe(&recipe).await {
                        Ok(_) => {
                            // Do nothing if no errors.
                        }
                        Err(spk_storage::Error::VersionExists(dest_recipe_ident)) => {
                            if self.allow_existing_version(&dest_recipe_ident).await? {
                                // The existing version was generated and published by a
                                // conversion process, e.g. spk convert pip/spk-convert-pip,
                                // so allow these builds to be published.
                                tracing::info!("Package version exists, allow-existing-with-label specified, and matched: publishing new builds allowed");
                            } else {
                                match pkg.build() {
                                    Some(_) => (), // If build provided, we can silently fail.
                                    None => {
                                        return Err(format!(
                                            "Failed to publish recipe {}: Version exists",
                                            recipe.ident(),
                                        )
                                        .into());
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            return Err(format!(
                                "Failed to publish recipe {}: {err}",
                                recipe.ident()
                            )
                            .into());
                        }
                    }
                }
            }
        }

        let builds = match pkg.build() {
            None => {
                with_cache_policy!(self.from, CachePolicy::BypassCache, {
                    self.from.list_package_builds(recipe_ident)
                })
                .await?
            }
            Some(build) => vec![pkg.to_build_ident(build.clone())],
        };

        for build in builds.iter() {
            use storage::RepositoryHandle::{SPFSWithVerbatimTags, SPFS};

            if build.is_source() && self.skip_source_packages {
                tracing::info!("skipping source package: {}", build.format_ident());
                continue;
            }

            if build.is_embedded() {
                // Don't attempt to publish an embedded package; the stub
                // will be recreated when publishing its provider.
                continue;
            }

            tracing::debug!("   loading package: {}", build.format_ident());
            let spec = self.from.read_package(build).await?;
            let components = self.from.read_components(build).await?;
            tracing::info!("publishing package: {}", spec.ident().format_ident());
            let env_spec = components.values().cloned().collect();
            tracing::debug!(
                " syncing components: {}",
                ComponentSet::from(components.keys().cloned()).format_components()
            );
            let syncer = match (&*self.from, &*self.to) {
                (SPFS(src), SPFS(dest)) => spfs::Syncer::new(src, dest),
                (SPFS(src), SPFSWithVerbatimTags(dest)) => spfs::Syncer::new(src, dest),
                (SPFSWithVerbatimTags(src), SPFS(dest)) => spfs::Syncer::new(src, dest),
                (SPFSWithVerbatimTags(src), SPFSWithVerbatimTags(dest)) => {
                    spfs::Syncer::new(src, dest)
                }
                _ => {
                    return Err(Error::String(
                        "Source and destination must both be spfs repositories".into(),
                    ))
                }
            };
            syncer
                .with_reporter(spfs::sync::reporter::SyncReporters::console())
                .sync_env(env_spec)
                .await?;
            self.to.publish_package(&spec, &components).await?;
        }

        Ok(builds)
    }
}
