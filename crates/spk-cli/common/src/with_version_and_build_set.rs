// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::Result;
use spk_schema::version_range::{EqualsVersion, VersionFilter};
use spk_schema::{AnyIdent, VersionIdent};
use spk_solve::solution::find_highest_package_version;
use spk_solve::{PkgRequest, RepositoryHandle};

use crate::Error;

/// Enum for the strategy to use to select the version in the
/// WithVersionSet trait if the version is not set.
pub enum DefaultVersionStrategy {
    Highest,
}

/// Enum for the strategy to use to select the build in the
/// WithVersionAndBuildSet trait if the build is not set.
pub enum DefaultBuildStrategy {
    First,
    Last,
}

/// Trait for ensuring a version is set
#[async_trait::async_trait]
pub trait WithVersionSet {
    type Output;

    /// If this does not have a version, find a version available for the
    /// package in the repos, according to the selection strategy, and
    /// return a request for that version. Otherwise the request contains
    /// a version, so return the request as it is.
    async fn with_version_or_else(
        &self,
        select_version_by: DefaultVersionStrategy,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Self::Output>;
}

/// Trait for ensuring a version and a build are set
#[async_trait::async_trait]
pub trait WithVersionAndBuildSet {
    type Output;

    /// If this does not have a version or build, find a version and build
    /// available for the package in the repos, according to the selection
    /// strategies, and return a request for that version and build.
    /// Otherwise the request contains a version and a build, so return
    /// the request as it is.
    async fn with_version_and_build_or_else(
        &self,
        select_version_by: DefaultVersionStrategy,
        select_build_by: DefaultBuildStrategy,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Self::Output>;
}

#[async_trait::async_trait]
impl WithVersionSet for PkgRequest {
    type Output = PkgRequest;

    async fn with_version_or_else(
        &self,
        select_version_by: DefaultVersionStrategy,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<PkgRequest> {
        if self.pkg.version.is_empty() {
            let selected_version = match select_version_by {
                DefaultVersionStrategy::Highest => {
                    find_highest_package_version(self.pkg.name.clone(), repos).await?
                }
            };
            let mut new_request = self.clone();
            new_request.pkg.version =
                VersionFilter::single(EqualsVersion::version_range((*selected_version).clone()));
            Ok(new_request)
        } else {
            Ok(self.clone())
        }
    }
}

#[async_trait::async_trait]
impl WithVersionAndBuildSet for PkgRequest {
    type Output = PkgRequest;

    async fn with_version_and_build_or_else(
        &self,
        select_version_by: DefaultVersionStrategy,
        select_build_by: DefaultBuildStrategy,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<PkgRequest> {
        if self.pkg.build.is_some() {
            Ok(self.clone())
        } else {
            // Has no build, need to make sure it has a version first
            let mut new_request = self.with_version_or_else(select_version_by, repos).await?;

            // This grabs a build for the version, from the first repo
            // that has a build for the version in it.
            let temp_ident: AnyIdent = new_request.pkg.clone().try_into()?;
            let ident: VersionIdent = temp_ident.to_version();

            for repo in repos {
                match select_build_by {
                    DefaultBuildStrategy::First => {
                        if let Some(pkg_ident) =
                            (repo.list_package_builds(&ident).await?).into_iter().next()
                        {
                            new_request.pkg.build = Some(pkg_ident.build().clone());
                            break;
                        }
                    }
                    DefaultBuildStrategy::Last => {
                        if let Some(pkg_ident) =
                            (repo.list_package_builds(&ident).await?).into_iter().last()
                        {
                            new_request.pkg.build = Some(pkg_ident.build().clone());
                            break;
                        }
                    }
                }
            }
            // This could have been given a request for a version of a
            // package that doesn't exist in any of the repos.
            if new_request.pkg.build.is_none() {
                return Err(Error::String(format!(
                    "There is no build for the package/version {new_request} in the repos: {}",
                    repos
                        .iter()
                        .map(|r| r.name().to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                ))
                .into());
            }
            Ok(new_request)
        }
    }
}
