// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::{exceptions::PyException, prelude::*};

pyo3::create_exception!(test, Error, PyException);

pub use Error as TestError;

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<super::PackageBuildTester>()?;
    m.add_class::<super::PackageInstallTester>()?;
    m.add_class::<super::PackageSourceTester>()?;
    m.add("TestError", py.get_type::<Error>())?;
    Ok(())
}
