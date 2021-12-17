// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use relative_path::{RelativePath, RelativePathBuf};
use spfs::prelude::Encodable;

use super::env::data_path;
use crate::{api, storage, Error, Result};

#[cfg(test)]
#[path = "./sources_test.rs"]
mod sources_test;

/// Denotes an error during the build process.
#[derive(Debug)]
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
/// ```
/// SourcePackageBuilder
///    .from_spec(api.Spec.from_dict({
///        "pkg": "my-pkg",
///     }))
///    .build()
///    .unwrap()
/// ```
#[pyclass]
pub struct SourcePackageBuilder {
    spec: api::Spec,
    repo: Option<storage::RepositoryHandle>,
    prefix: PathBuf,
}

#[pymethods]
impl SourcePackageBuilder {
    #[staticmethod]
    pub fn from_spec(mut spec: api::Spec) -> Self {
        spec.pkg = spec.pkg.with_build(Some(api::Build::Source));
        Self {
            spec,
            repo: None,
            prefix: PathBuf::from("/spfs"),
        }
    }

    #[pyo3(name = "build")]
    fn build_py(&mut self) -> Result<api::Ident> {
        // build is intended to consume the builder,
        // but we cannot effectively do this from
        // a python reference. So we make a partial
        // clone/copy with the assumption that python
        // won't reuse this builder
        Self {
            spec: self.spec.clone(),
            prefix: self.prefix.clone(),
            repo: self.repo.take(),
        }
        .build()
    }
}

impl SourcePackageBuilder {
    /// Set the repository that the created package should be published to.
    pub fn with_target_repository(&mut self, repo: storage::RepositoryHandle) -> &mut Self {
        self.repo = Some(repo);
        self
    }

    /// Build the requested source package.
    pub fn build(mut self) -> Result<api::Ident> {
        let layer = self.collect_and_commit_sources()?;
        let repo = match &mut self.repo {
            Some(r) => r,
            None => {
                let repo = storage::local_repository()?;
                self.repo.insert(repo.into())
            }
        };
        let pkg = self.spec.pkg.clone();
        repo.publish_package(self.spec, layer.digest()?)?;
        Ok(pkg)
    }

    /// Collect sources for the given spec and commit them into an spfs layer.
    fn collect_and_commit_sources(&self) -> Result<spfs::graph::Layer> {
        let mut runtime = spfs::active_runtime()?;
        runtime.reset_stack()?;
        runtime.reset_all()?;
        runtime.set_editable(true)?;
        spfs::remount_runtime(&runtime)?;

        let source_dir = data_path(&self.spec.pkg, &self.prefix);
        collect_sources(&self.spec, &source_dir)?;

        tracing::info!("Validating package source files...");
        let diffs = spfs::diff(None, None)?;
        validate_source_changeset(
            diffs,
            RelativePathBuf::from(source_dir.to_string_lossy().to_string()),
        )?;

        Ok(spfs::commit_layer(&mut runtime)?)
    }
}

/// Collect the sources for a spec in the given directory.
fn collect_sources<P: AsRef<Path>>(spec: &api::Spec, source_dir: P) -> Result<()> {
    let source_dir = source_dir.as_ref();
    std::fs::create_dir_all(&source_dir)?;

    let original_env = std::env::vars();
    super::binary::get_package_build_env(spec)
        .into_iter()
        .map(|(n, v)| std::env::set_var(n, v))
        .count();
    let mut res = Ok(());
    for source in spec.sources.iter() {
        let target_dir = match source.subdir() {
            Some(subdir) => subdir.to_path(source_dir),
            None => source_dir.into(),
        };
        res = std::fs::create_dir_all(&target_dir)
            .map_err(Error::from)
            .and_then(|_| source.collect(&target_dir));
        if res.is_err() {
            break;
        }
    }
    std::env::vars()
        .map(|(n, _)| n)
        .map(std::env::remove_var)
        .count();
    original_env.map(|(n, v)| std::env::set_var(n, v)).count();
    res
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
        if diff.mode == spfs::tracking::DiffMode::Unchanged {
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
