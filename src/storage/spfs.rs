use pyo3::prelude::*;
use spfs;

use crate::Result;

#[pyclass(subclass)]
pub struct SpFSRepository {
    inner: spfs::storage::RepositoryHandle,
}

#[pymethods]
impl SpFSRepository {
    #[new]
    pub fn new(address: &str) -> crate::Result<Self> {
        Ok(Self {
            inner: spfs::storage::open_repository(address)?,
        })
    }
}

/// Return the local packages repository used for development.
pub fn local_repository() -> Result<SpFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_repository()?;
    Ok(SpFSRepository { inner: repo.into() })
}

/// Return the remote repository of the given name.
///
/// If not name is specified, return the default spfs repository.
pub fn remote_repository<S: AsRef<str>>(name: S) -> Result<SpFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_remote(name)?;
    Ok(SpFSRepository { inner: repo })
}
