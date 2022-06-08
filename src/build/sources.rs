// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use thiserror::Error;

use relative_path::{RelativePath, RelativePathBuf};
use spfs::prelude::Encodable;

use super::env::data_path;
use crate::{api, prelude::*, storage, Error, Result};

#[cfg(test)]
#[path = "./sources_test.rs"]
mod sources_test;

/// Denotes an error during the build process.
#[derive(Debug, Error)]
#[error("Collection error: {message}")]
pub struct CollectionError {
    pub message: String,
}

impl CollectionError {
    pub fn new_error(format_args: std::fmt::Arguments) -> crate::Error {
        crate::Error::Collection(Self {
            message: std::fmt::format(format_args),
        })
    }
}

/// Builds a source package.
///
/// ```no_run
/// # #[macro_use] extern crate spk;
/// # async fn demo() {
/// spk::build::SourcePackageBuilder::from_recipe(spk::recipe!({
///        "pkg": "my-pkg",
///     }))
///    .build()
///    .await
///    .unwrap();
/// # }
/// ```
pub struct SourcePackageBuilder<Recipe: api::Recipe> {
    recipe: Recipe,
    prefix: PathBuf,
}

impl<Recipe: api::Recipe> SourcePackageBuilder<Recipe> {
    pub fn from_recipe(recipe: Recipe) -> Self {
        Self {
            recipe,
            prefix: PathBuf::from("/spfs"),
        }
    }

    pub async fn build_and_publish<R, T>(
        &mut self,
        repo: &R,
    ) -> Result<(
        Recipe::Output,
        HashMap<api::Component, spfs::encoding::Digest>,
    )>
    where
        R: std::ops::Deref<Target = T>,
        T: storage::Repository<Recipe = Recipe> + ?Sized,
    {
        let (package, components) = self.build().await?;
        repo.publish_package(&package, &components).await?;
        Ok((package, components))
    }

    /// Build the requested source package.
    pub async fn build(
        &self,
    ) -> Result<(
        Recipe::Output,
        HashMap<api::Component, spfs::encoding::Digest>,
    )> {
        let package = self.recipe.generate_source_build()?;
        let layer = self.collect_and_commit_sources(&package).await?;
        if !package.ident().is_source() {
            return Err(Error::String(format!(
                "Recipe generate source package with non-source identifier {}",
                package.ident()
            )));
        }
        let mut components = std::collections::HashMap::with_capacity(1);
        components.insert(api::Component::Source, layer.digest()?);
        Ok((package, components))
    }

    /// Collect sources for the given spec and commit them into an spfs layer.
    async fn collect_and_commit_sources(
        &self,
        package: &Recipe::Output,
    ) -> Result<spfs::graph::Layer> {
        let repo = spfs::get_config()?.get_local_repository_handle().await?;
        let mut runtime = spfs::active_runtime().await?;
        runtime.reset_all()?;
        runtime.status.editable = true;
        runtime.status.stack.clear();
        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;

        let source_dir = data_path(package.ident()).to_path(&self.prefix);
        collect_sources(package, &source_dir)?;

        tracing::info!("Validating source package contents...");
        let diffs = spfs::diff(None, None).await?;
        validate_source_changeset(
            diffs,
            RelativePathBuf::from(source_dir.to_string_lossy().to_string()),
        )?;

        tracing::info!("Committing source package contents...");
        Ok(spfs::commit_layer(&mut runtime, repo.into()).await?)
    }
}

/// Collect the sources for a spec in the given directory.
pub(super) fn collect_sources<P: AsRef<Path>>(spec: &impl Package, source_dir: P) -> Result<()> {
    let source_dir = source_dir.as_ref();
    std::fs::create_dir_all(&source_dir)?;

    let env = super::binary::get_package_build_env(spec);
    for source in spec.sources().iter() {
        let target_dir = match source.subdir() {
            Some(subdir) => subdir.to_path(source_dir),
            None => source_dir.into(),
        };
        std::fs::create_dir_all(&target_dir)?;
        source.collect(&target_dir, &env).map_err(|err| {
            CollectionError::new_error(format_args!(
                "Failed to collect source: {}\n{:?}",
                err, source
            ))
        })?;
    }
    Ok(())
}

/// Validate the set of diffs for a source package build.
///
/// # Errors:
///   - CollectionError: if any issues are identified in the changeset
pub fn validate_source_changeset<P: AsRef<RelativePath>>(
    diffs: Vec<spfs::tracking::Diff>,
    source_dir: P,
) -> Result<()> {
    if diffs.is_empty() {
        return Err(CollectionError::new_error(format_args!(
            "No source files collected, source package would be empty"
        )));
    }

    let mut source_dir = source_dir.as_ref();
    source_dir = source_dir.strip_prefix("/spfs").unwrap_or(source_dir);
    for diff in diffs.into_iter() {
        if diff.mode.is_unchanged() {
            continue;
        }
        if diff.path.starts_with(&source_dir) {
            // the change is within the source directory
            continue;
        }
        if source_dir.starts_with(&diff.path) {
            // the path is to a parent directory of the source path
            continue;
        }
        return Err(CollectionError::new_error(format_args!(
            "Invalid source file path found: {} (not under {})",
            &diff.path, source_dir
        )));
    }
    Ok(())
}
