// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{prelude::*, wrap_pyfunction};

use crate::{api, Result};

pyo3::create_exception!(build, BuildError, pyo3::exceptions::PyRuntimeError);
pyo3::create_exception!(build, CollectionError, BuildError);

#[pyfunction]
fn validate_source_changeset() -> Result<()> {
    let diffs = spfs::diff(None, None)?;
    super::validate_source_changeset(diffs, "/spfs")?;
    Ok(())
}

#[pyfunction]
fn build_options_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_options_path(path, prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn build_script_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_script_path(path, prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn build_spec_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_spec_path(path, prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn source_package_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::source_package_path(path, prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate_source_changeset, m)?)?;
    m.add_function(wrap_pyfunction!(validate_source_changeset, m)?)?;
    m.add_function(wrap_pyfunction!(build_options_path, m)?)?;
    m.add_function(wrap_pyfunction!(build_script_path, m)?)?;
    m.add_function(wrap_pyfunction!(build_spec_path, m)?)?;
    m.add_function(wrap_pyfunction!(source_package_path, m)?)?;

    m.add_class::<super::BinaryPackageBuilder>()?;
    m.add_class::<super::SourcePackageBuilder>()?;
    m.add("BuildError", py.get_type::<BuildError>())?;
    m.add("CollectionError", py.get_type::<CollectionError>())?;
    Ok(())
}
