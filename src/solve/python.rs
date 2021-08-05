// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use pyo3::py_run;

use super::errors::SolverError;
use super::graph::{
    Change, Decision, Graph, Node, Note, RequestPackage, RequestVar, SetOptions, SetPackage,
    SetPackageBuild, SkipPackageNote, StepBack,
};
use super::solution::Solution;
use super::solver::{Solver, SolverFailedError};
use super::validation::Validator;

fn init_submodule_errors(py: &Python, module: &PyModule) -> PyResult<()> {
    module.add("SolverError", py.get_type::<SolverError>())?;
    Ok(())
}

fn init_submodule_graph(_py: &Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<Change>()?;
    module.add_class::<Decision>()?;
    module.add_class::<Graph>()?;
    module.add_class::<Node>()?;
    module.add_class::<Note>()?;
    module.add_class::<RequestPackage>()?;
    module.add_class::<RequestVar>()?;
    module.add_class::<SetOptions>()?;
    module.add_class::<SetPackage>()?;
    module.add_class::<SetPackageBuild>()?;
    module.add_class::<SkipPackageNote>()?;
    module.add_class::<StepBack>()?;
    Ok(())
}

fn init_submodule_solution(_py: &Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<Solution>()?;
    Ok(())
}

fn init_submodule_solver(py: &Python, module: &PyModule) -> PyResult<()> {
    module.add("SolverFailedError", py.get_type::<SolverFailedError>())?;
    module.add_class::<Solver>()?;
    Ok(())
}

fn init_submodule_validation(_py: &Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<Validator>()?;
    Ok(())
}

macro_rules! add_submodule {
    ($m:ident, $py:ident, $mod_name:expr, $init_fn:ident) => {
        let submod = PyModule::new(*$py, $mod_name)?;
        // Hack to make `from spk.solve.foo import ...` work
        py_run!(
            *$py,
            submod,
            &format!(
                "import sys; sys.modules['spkrs.solve.{}'] = submod",
                $mod_name
            )
        );
        $init_fn($py, submod)?;
        $m.add_submodule(submod)?;
    };
}

pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    add_submodule!(m, py, "_errors", init_submodule_errors);
    add_submodule!(m, py, "graph", init_submodule_graph);
    add_submodule!(m, py, "_solver", init_submodule_solver);
    add_submodule!(m, py, "_solution", init_submodule_solution);
    add_submodule!(m, py, "validation", init_submodule_validation);

    m.add_class::<Graph>()?;
    m.add_class::<Solution>()?;
    m.add_class::<Solver>()?;

    m.add("SolverError", py.get_type::<SolverError>())?;
    m.add("SolverFailedError", py.get_type::<SolverFailedError>())?;

    Ok(())
}
