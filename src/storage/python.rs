// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{exceptions, prelude::*, wrap_pyfunction};

use crate::{api, storage::RepositoryHandle, Result};

#[pyfunction]
fn local_repository() -> Result<Repository> {
    Ok(super::local_repository().map(|r| Repository {
        handle: RepositoryHandle::SPFS(r),
    })?)
}

#[pyfunction(path = "\"origin\"")]
fn remote_repository(path: &str) -> Result<Repository> {
    Ok(super::remote_repository(path).map(|r| Repository {
        handle: RepositoryHandle::SPFS(r),
    })?)
}

#[pyfunction]
fn open_tar_repository(path: &str, create: Option<bool>) -> Result<Repository> {
    let repo = match create {
        Some(true) => spfs::storage::tar::TarRepository::create(path)?,
        _ => spfs::storage::tar::TarRepository::open(path)?,
    };
    let handle: spfs::storage::RepositoryHandle = repo.into();
    Ok(Repository {
        handle: RepositoryHandle::from(super::SPFSRepository::from(handle)),
    })
}

#[pyfunction]
fn open_spfs_repository(path: &str, create: Option<bool>) -> Result<Repository> {
    let repo = match create {
        Some(true) => spfs::storage::fs::FSRepository::create(path)?,
        _ => spfs::storage::fs::FSRepository::open(path)?,
    };
    let handle: spfs::storage::RepositoryHandle = repo.into();
    Ok(Repository {
        handle: RepositoryHandle::from(super::SPFSRepository::from(handle)),
    })
}

#[pyfunction]
fn mem_repository() -> Repository {
    Repository {
        handle: RepositoryHandle::Mem(Default::default()),
    }
}

#[pyfunction]
fn runtime_repository() -> Repository {
    Repository {
        handle: RepositoryHandle::Runtime(Default::default()),
    }
}

#[pyfunction]
fn export_package(pkg: &api::Ident, filename: &str) -> Result<()> {
    super::export_package(pkg, filename)
}

#[pyfunction]
fn import_package(filename: &str) -> Result<()> {
    super::import_package(filename)
}

#[pyclass]
struct Repository {
    handle: RepositoryHandle,
}

#[pymethods]
impl Repository {
    fn is_spfs(&self) -> bool {
        if let RepositoryHandle::SPFS(_) = self.handle {
            true
        } else {
            false
        }
    }
    fn list_packages(&self) -> Result<Vec<String>> {
        self.handle.list_packages()
    }
    fn list_package_versions(&self, name: &str) -> Result<Vec<String>> {
        self.handle.list_package_versions(name)
    }
    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        self.handle.list_package_builds(pkg)
    }
    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        self.handle.read_spec(pkg)
    }
    fn get_package(&self, pkg: &api::Ident) -> Result<crate::Digest> {
        self.handle
            .get_package(pkg)
            .map(|d| crate::Digest { inner: d })
    }
    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        self.handle.publish_spec(spec)
    }
    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()> {
        self.handle.remove_spec(pkg)
    }
    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        self.handle.force_publish_spec(spec)
    }
    fn publish_package(&mut self, spec: api::Spec, digest: crate::Digest) -> Result<()> {
        self.handle.publish_package(spec, digest.inner)
    }
    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        self.handle.remove_package(pkg)
    }
    pub fn has_digest(&self, digest: &crate::Digest) -> Result<bool> {
        if let RepositoryHandle::SPFS(repo) = &self.handle {
            Ok(repo.has_digest(&digest))
        } else {
            Err(crate::Error::PyErr(exceptions::PyValueError::new_err(
                "Not an spfs repository",
            )))
        }
    }
    pub fn push_digest(&self, digest: &crate::Digest, dest: &mut Self) -> Result<()> {
        match (&self.handle, &mut dest.handle) {
            (RepositoryHandle::SPFS(src), RepositoryHandle::SPFS(dest)) => {
                src.push_digest(&digest, dest)
            }
            _ => Err(crate::Error::PyErr(exceptions::PyValueError::new_err(
                "Source and dest must both be spfs repositories",
            ))),
        }
    }
    pub fn localize_digest(&self, digest: &crate::Digest) -> Result<()> {
        if let RepositoryHandle::SPFS(repo) = &self.handle {
            repo.localize_digest(&digest)
        } else {
            Err(crate::Error::PyErr(exceptions::PyValueError::new_err(
                "Not an spfs repository",
            )))
        }
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
