// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use pyo3::prelude::*;

use crate::{api, io, storage, Error, Result};

#[cfg(test)]
#[path = "./publish_test.rs"]
mod publish_test;

#[pyclass]
pub struct Publisher {
    from: Arc<storage::RepositoryHandle>,
    to: Arc<storage::RepositoryHandle>,
    skip_source_packages: bool,
    force: bool,
}

impl Publisher {
    fn new(
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
    pub fn with_source(&mut self, repo: Arc<storage::RepositoryHandle>) -> &mut Self {
        self.from = repo;
        self
    }

    /// Change the destination repository to publish packages into.
    pub fn with_target(&mut self, repo: Arc<storage::RepositoryHandle>) -> &mut Self {
        self.to = repo;
        self
    }

    /// Do not publish source packages, even if they exist for the version being published.
    pub fn skip_source_packages(&mut self, skip_source_packages: bool) -> &mut Self {
        self.skip_source_packages = skip_source_packages;
        self
    }

    /// Forcefully publishing a package will overwrite an existing publish if it exists.
    pub fn force(&mut self, force: bool) -> &mut Self {
        self.force = force;
        self
    }
}

#[pymethods]
impl Publisher {
    #[new]
    fn new_py() -> Result<Self> {
        let from = Arc::new(crate::HANDLE.block_on(storage::local_repository())?.into());
        let to = Arc::new(
            crate::HANDLE
                .block_on(storage::remote_repository("origin"))?
                .into(),
        );
        Ok(Self::new(from, to))
    }

    #[pyo3(name = "with_source")]
    pub fn with_source_py(
        mut slf: PyRefMut<Self>,
        repo: storage::python::Repository,
    ) -> PyRefMut<Self> {
        slf.from = repo.handle;
        slf
    }

    #[pyo3(name = "with_target")]
    pub fn with_target_py(
        mut slf: PyRefMut<Self>,
        repo: storage::python::Repository,
    ) -> PyRefMut<Self> {
        slf.to = repo.handle;
        slf
    }

    #[pyo3(name = "skip_source_packages")]
    pub fn skip_source_packages_py(
        mut slf: PyRefMut<Self>,
        skip_source_packages: bool,
    ) -> PyRefMut<Self> {
        slf.skip_source_packages = skip_source_packages;
        slf
    }

    #[pyo3(name = "force")]
    pub fn force_py(mut slf: PyRefMut<Self>, force: bool) -> PyRefMut<Self> {
        slf.force = force;
        slf
    }

    pub fn publish(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        let builds = if pkg.build.is_none() {
            tracing::info!("   loading spec: {}", io::format_ident(pkg));
            match self.from.read_spec(pkg) {
                Err(Error::PackageNotFoundError(_)) => (),
                Err(err) => return Err(err),
                Ok(spec) => {
                    tracing::info!("publishing spec: {}", io::format_ident(&spec.pkg));
                    if self.force {
                        self.to.force_publish_spec(spec)?;
                    } else {
                        self.to.publish_spec(spec)?;
                    }
                }
            }

            self.from
                .list_package_builds(pkg)?
                .into_iter()
                .map(Into::into)
                .collect()
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
            let spec = self.from.read_spec(build)?;
            let components = self.from.get_package(build)?;
            tracing::info!("publishing package: {}", io::format_ident(&spec.pkg));
            match (&*self.from, &*self.to) {
                (SPFS(src), SPFS(dest)) => {
                    for (name, digest) in components.iter() {
                        tracing::debug!(" syncing component: {}", io::format_components([name]));
                        crate::HANDLE.block_on(spfs::sync_ref(digest.to_string(), src, dest))?;
                    }
                }
                _ => {
                    return Err(Error::String(
                        "Source and destination must both be spfs repositories".into(),
                    ))
                }
            }
            self.to.publish_package(spec, components)?;
        }

        Ok(builds)
    }
}
