// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::sync::Arc;

use pyo3::{exceptions, prelude::*, wrap_pyfunction};

use crate::{api, storage::RepositoryHandle, Result};

#[pyfunction]
fn local_repository() -> Result<Repository> {
    crate::HANDLE
        .block_on(super::local_repository())
        .map(|r| Repository {
            handle: Arc::new(RepositoryHandle::SPFS(r)),
        })
}

#[pyfunction(path = "\"origin\"")]
fn remote_repository(path: &str) -> Result<Repository> {
    crate::HANDLE
        .block_on(super::remote_repository(path))
        .map(|r| Repository {
            handle: Arc::new(RepositoryHandle::SPFS(r)),
        })
}

#[pyfunction]
fn open_tar_repository(path: &str, create: Option<bool>) -> Result<Repository> {
    let repo = match create {
        Some(true) => crate::HANDLE.block_on(spfs::storage::tar::TarRepository::create(path))?,
        _ => crate::HANDLE.block_on(spfs::storage::tar::TarRepository::open(path))?,
    };
    let handle: spfs::storage::RepositoryHandle = repo.into();
    Ok(Repository {
        handle: Arc::new(RepositoryHandle::from(super::SPFSRepository::from(handle))),
    })
}

#[pyfunction]
fn open_spfs_repository(path: &str, create: Option<bool>) -> Result<Repository> {
    let repo = match create {
        Some(true) => crate::HANDLE.block_on(spfs::storage::fs::FSRepository::create(path))?,
        _ => crate::HANDLE.block_on(spfs::storage::fs::FSRepository::open(path))?,
    };
    let handle: spfs::storage::RepositoryHandle = repo.into();
    Ok(Repository {
        handle: Arc::new(RepositoryHandle::from(super::SPFSRepository::from(handle))),
    })
}

#[pyfunction]
fn mem_repository() -> Repository {
    Repository {
        handle: Arc::new(RepositoryHandle::Mem(Default::default())),
    }
}

#[pyfunction]
fn runtime_repository() -> Repository {
    Repository {
        handle: Arc::new(RepositoryHandle::Runtime(Default::default())),
    }
}

#[pyfunction]
fn export_package(pkg: &api::Ident, filename: &str) -> Result<()> {
    let _guard = crate::HANDLE.enter();
    super::export_package(pkg, filename)
}

#[pyfunction]
fn import_package(filename: &str) -> Result<()> {
    crate::HANDLE.block_on(super::import_package(filename))
}

#[pyclass]
#[derive(Clone)]
pub struct Repository {
    pub handle: Arc<RepositoryHandle>,
}

#[pymethods]
impl Repository {
    fn is_spfs(&self) -> bool {
        let _guard = crate::HANDLE.enter();
        self.handle.is_spfs()
    }
    fn list_packages(&self) -> Result<Vec<String>> {
        let _guard = crate::HANDLE.enter();
        self.handle.list_packages()
    }
    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        let _guard = crate::HANDLE.enter();
        self.handle.list_package_versions(name)
    }
    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::BuildIdent>> {
        let _guard = crate::HANDLE.enter();
        self.handle.list_package_builds(pkg)
    }
    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        let _guard = crate::HANDLE.enter();
        self.handle.list_build_components(pkg)
    }
    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        let _guard = crate::HANDLE.enter();
        self.handle.read_spec(pkg)
    }
    fn get_package(&self, pkg: &api::Ident) -> Result<HashMap<api::Component, crate::Digest>> {
        let _guard = crate::HANDLE.enter();
        Ok(self
            .handle
            .get_package(pkg)?
            .into_iter()
            .map(|(c, d)| (c, crate::Digest { inner: d }))
            .collect())
    }
    fn publish_spec(&self, spec: api::Spec) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        self.handle.publish_spec(spec)
    }
    fn remove_spec(&self, pkg: &api::Ident) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        self.handle.remove_spec(pkg)
    }
    fn force_publish_spec(&self, spec: api::Spec) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        self.handle.force_publish_spec(spec)
    }
    fn publish_package(
        &self,
        spec: api::Spec,
        components: HashMap<api::Component, crate::Digest>,
    ) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        let mapped = components.into_iter().map(|(c, d)| (c, d.inner)).collect();
        self.handle.publish_package(spec, mapped)
    }
    fn remove_package(&self, pkg: &api::Ident) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        self.handle.remove_package(pkg)
    }
    pub fn push_digest(&self, digest: &crate::Digest, dest: &mut Self) -> Result<()> {
        match (&*self.handle, &*dest.handle) {
            (RepositoryHandle::SPFS(src), RepositoryHandle::SPFS(dest)) => {
                crate::HANDLE.block_on(spfs::sync_ref(digest.inner.to_string(), src, dest))?;
                Ok(())
            }
            _ => Err(crate::Error::PyErr(exceptions::PyValueError::new_err(
                "Source and dest must both be spfs repositories",
            ))),
        }
    }
    pub fn upgrade(&mut self) -> Result<String> {
        let _guard = crate::HANDLE.enter();
        self.handle.upgrade()
    }
}

impl IntoPy<Repository> for RepositoryHandle {
    fn into_py(self, py: Python) -> Repository {
        Arc::new(self).into_py(py)
    }
}

impl IntoPy<Repository> for Arc<RepositoryHandle> {
    fn into_py(self, _py: Python) -> Repository {
        Repository { handle: self }
    }
}

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(local_repository, m)?)?;
    m.add_function(wrap_pyfunction!(remote_repository, m)?)?;
    m.add_function(wrap_pyfunction!(open_tar_repository, m)?)?;
    m.add_function(wrap_pyfunction!(open_spfs_repository, m)?)?;
    m.add_function(wrap_pyfunction!(mem_repository, m)?)?;
    m.add_function(wrap_pyfunction!(runtime_repository, m)?)?;
    m.add_function(wrap_pyfunction!(export_package, m)?)?;
    m.add_function(wrap_pyfunction!(import_package, m)?)?;

    m.add_class::<Repository>()?;

    Ok(())
}
