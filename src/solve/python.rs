// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use super::solver::Solver;

fn init_submodule_solver(module: &PyModule) -> PyResult<()> {
    module.add_class::<Solver>()?;
    Ok(())
}

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    {
        let submod_solver = PyModule::new(*py, "_solver")?;
        init_submodule_solver(submod_solver)?;
        m.add_submodule(submod_solver)?;
    }

    m.add_class::<Solver>()?;

    Ok(())
}
