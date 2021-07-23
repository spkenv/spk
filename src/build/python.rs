// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{prelude::*, wrap_pyfunction};

use crate::Result;

#[pyfunction]
fn validate_build_changeset() -> Result<()> {
    let diffs = spfs::diff(None, None)?;
    super::validate_build_changeset(diffs, "/spfs")?;
    Ok(())
}

#[pyfunction]
fn validate_source_changeset() -> Result<()> {
    let diffs = spfs::diff(None, None)?;
    super::validate_source_changeset(diffs, "/spfs")?;
    Ok(())
}

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate_build_changeset, m)?)?;
    m.add_function(wrap_pyfunction!(validate_source_changeset, m)?)?;
    Ok(())
}
