// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::{Path, PathBuf};

use relative_path::{RelativePath, RelativePathBuf};
use spfs::prelude::Encodable;

use super::env::data_path;
use crate::{api, storage, Result};

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
/// ``
pub struct SourcePackageBuilder<'spec> {
    spec: &'spec api::Spec,
    repo: Option<storage::RepositoryHandle>,
    prefix: PathBuf,
}

impl<'spec> SourcePackageBuilder<'spec> {
    pub fn from_spec(spec: &'spec api::Spec) -> Self {
        Self {
            spec,
            repo: None,
            prefix: PathBuf::from("/spfs"),
        }
    }

    /// Set the repository that the created package should be published to.
    pub fn with_target_repository(&mut self, repo: storage::RepositoryHandle) -> &mut Self {
        self.repo = Some(repo);
        self
    }

    /// Build the requested source package.
    pub fn build(&mut self) -> Result<api::Ident> {
        let layer = self.collect_and_commit_sources()?;
        let repo = match &mut self.repo {
            Some(r) => r,
            None => {
                let repo = storage::local_repository()?;
                self.repo.insert(repo.into())
            }
        };
        let mut spec = self.spec.clone();
        spec.pkg = spec.pkg.with_build(Some(api::Build::Source));
        let res = spec.pkg.clone();
        repo.publish_package(spec, layer.digest()?)?;
        Ok(res)
    }

    /// Collect sources for the given spec and commit them into an spfs layer.
    fn collect_and_commit_sources(&self) -> Result<spfs::graph::Layer> {
        let pkg = self.spec.pkg.with_build(Some(api::Build::Source));

        let mut runtime = spfs::active_runtime()?;
        runtime.reset_stack()?;
        runtime.reset_all()?;
        runtime.set_editable(true)?;
        spfs::remount_runtime(&runtime)?;

        let source_dir = data_path(&pkg, &self.prefix);
        collect_sources(self.spec, &source_dir)?;

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
    todo!()
    // os.makedirs(source_dir)

    // original_env = os.environ.copy()
    // os.environ.update(get_package_build_env(spec))
    // try:
    //     for source in spec.sources:
    //         target_dir = source_dir
    //         subdir = source.subdir
    //         if subdir:
    //             target_dir = os.path.join(source_dir, subdir.lstrip("/"))
    //         os.makedirs(target_dir, exist_ok=True)
    //         api.collect_source(source, target_dir)
    // finally:
    //     os.environ.clear()
    //     os.environ.update(original_env)
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
