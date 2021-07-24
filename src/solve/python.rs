// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use super::graph::Graph;
use super::solver::Solver;

fn init_submodule_graph(module: &PyModule) -> PyResult<()> {
    module.add_class::<Graph>()?;
    Ok(())
}

fn init_submodule_solver(module: &PyModule) -> PyResult<()> {
    module.add_class::<Solver>()?;
    Ok(())
}

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    {
        let submod_graph = PyModule::new(*py, "graph")?;
        init_submodule_graph(submod_graph)?;
        m.add_submodule(submod_graph)?;
    }
    {
        let submod_solver = PyModule::new(*py, "_solver")?;
        init_submodule_solver(submod_solver)?;
        m.add_submodule(submod_solver)?;
    }

    m.add_class::<Graph>()?;
    m.add_class::<Solver>()?;

    Ok(())
}
