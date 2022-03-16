// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{collections::HashMap, path::PathBuf};

use pyo3::{prelude::*, wrap_pyfunction};

use crate::{api, Result};

pyo3::create_exception!(build, BuildError, pyo3::exceptions::PyRuntimeError);
pyo3::create_exception!(build, CollectionError, BuildError);

#[pyfunction]
fn validate_source_changeset() -> Result<()> {
    let diffs = crate::HANDLE.block_on(spfs::diff(None, None))?;
    super::validate_source_changeset(diffs, "/spfs")?;
    Ok(())
}

#[pyfunction]
fn build_options_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_options_path(path)
        .to_path(prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn build_script_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_script_path(path)
        .to_path(prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn build_spec_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::build_spec_path(path)
        .to_path(prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
fn source_package_path(path: &api::Ident, prefix: Option<&str>) -> String {
    super::source_package_path(path)
        .to_path(prefix.unwrap_or("/spfs"))
        .to_string_lossy()
        .to_string()
}

#[pyfunction]
pub fn get_package_build_env(spec: &api::Spec) -> HashMap<String, String> {
    super::get_package_build_env(spec)
}

#[pyfunction]
pub fn data_path(pkg: &api::Ident, prefix: PathBuf) -> PathBuf {
    super::env::data_path(pkg).to_path(prefix)
}

#[pyfunction]
pub fn collect_sources(spec: &api::Spec, source_dir: PathBuf) -> Result<()> {
    super::sources::collect_sources(spec, source_dir)
}

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate_source_changeset, m)?)?;
    m.add_function(wrap_pyfunction!(validate_source_changeset, m)?)?;
    m.add_function(wrap_pyfunction!(build_options_path, m)?)?;
    m.add_function(wrap_pyfunction!(build_script_path, m)?)?;
    m.add_function(wrap_pyfunction!(build_spec_path, m)?)?;
    m.add_function(wrap_pyfunction!(source_package_path, m)?)?;
    m.add_function(wrap_pyfunction!(get_package_build_env, m)?)?;
    m.add_function(wrap_pyfunction!(data_path, m)?)?;
    m.add_function(wrap_pyfunction!(collect_sources, m)?)?;

    m.add_class::<super::BinaryPackageBuilder>()?;
    m.add_class::<super::SourcePackageBuilder>()?;
    m.add("BuildError", py.get_type::<BuildError>())?;
    m.add("CollectionError", py.get_type::<CollectionError>())?;
    Ok(())
}
