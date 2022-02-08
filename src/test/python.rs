// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::prelude::*;

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<super::PackageBuildTester>()?;
    m.add_class::<super::PackageInstallTester>()?;
    m.add_class::<super::PackageSourceTester>()?;
    Ok(())
}
