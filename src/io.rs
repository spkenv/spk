// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;

use crate::api;

pub fn format_ident(pkg: &api::Ident) -> String {
    let mut out = pkg.name().bold().to_string();
    if !pkg.version.is_zero() || pkg.build.is_some() {
        out = format!("{}/{}", out, pkg.version.to_string().bright_blue());
    }
    if let Some(ref b) = pkg.build {
        out = format!("{}/{}", out, format_build(b));
    }
    out
}

pub fn format_build(build: &api::Build) -> String {
    match build {
        api::Build::Embedded => build.digest().bright_magenta().to_string(),
        api::Build::Source => build.digest().bright_yellow().to_string(),
        _ => build.digest().dimmed().to_string(),
    }
}

pub mod python {
    use crate::api;
    use pyo3::prelude::*;

    #[pyfunction]
    pub fn format_ident(pkg: &api::Ident) -> String {
        super::format_ident(pkg)
    }
    #[pyfunction]
    pub fn format_build(build: api::Build) -> String {
        super::format_build(&build)
    }

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(format_ident, m)?)?;
        m.add_function(wrap_pyfunction!(format_build, m)?)?;
        Ok(())
    }
}
