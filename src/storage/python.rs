// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{prelude::*, wrap_pyfunction};

use crate::Result;

#[pyfunction]
fn local_repository() -> Result<super::SpFSRepository> {
    Ok(super::local_repository()?)
}

#[pyfunction]
fn remote_repository(path: &str) -> Result<super::SpFSRepository> {
    Ok(super::remote_repository(path)?)
}

#[pyfunction]
fn open_tar_repository(path: &str, create: Option<bool>) -> Result<super::SpFSRepository> {
    let repo = match create {
        Some(true) => spfs::storage::tar::TarRepository::create(path)?,
        _ => spfs::storage::tar::TarRepository::open(path)?,
    };
    let handle: spfs::storage::RepositoryHandle = repo.into();
    Ok(super::SpFSRepository::from(handle))
}

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(local_repository, m)?)?;
    m.add_function(wrap_pyfunction!(remote_repository, m)?)?;
    m.add_function(wrap_pyfunction!(open_tar_repository, m)?)?;
    Ok(())
}
