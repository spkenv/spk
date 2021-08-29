// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use pyo3::py_run;
use pyo3::wrap_pyfunction;
use std::sync::Arc;

use crate::api;

use super::errors::{PackageNotFoundError, SolverError};
use super::graph::{
    Change, Decision, Graph, Node, Note, RequestPackage, RequestVar, SetOptions, SetPackage,
    SetPackageBuild, SkipPackageNote, State, StepBack,
};
use super::solution::{PackageSource, Solution};
use super::solver::{Solver, SolverFailedError};
use super::validation::{self, Validators, VarRequirementsValidator};

fn init_submodule_errors(py: &Python, module: &PyModule) -> PyResult<()> {
    module.add(
        "PackageNotFoundError",
        py.get_type::<PackageNotFoundError>(),
    )?;
    module.add("SolverError", py.get_type::<SolverError>())?;
    Ok(())
}

#[pyfunction]
#[pyo3(name = "BuildPackage")]
fn build_package(
    spec: &api::Spec,
    source: PackageSource,
    build_env: &Solution,
) -> crate::Result<Decision> {
    super::graph::Decision::build_package(Arc::new(spec.clone()), &source, build_env)
}

#[pyfunction]
#[pyo3(name = "ResolvePackage")]
fn resolve_package(spec: &api::Spec, source: PackageSource) -> Decision {
    super::graph::Decision::resolve_package(spec, source)
}

fn init_submodule_graph(_py: &Python, module: &PyModule) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(build_package, module)?)?;
    module.add_function(wrap_pyfunction!(resolve_package, module)?)?;

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
    module.add_class::<State>()?;
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

#[pyfunction]
fn default_validators() -> Vec<Validators> {
    validation::default_validators()
}

fn init_submodule_validation(_py: &Python, module: &PyModule) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(default_validators, module)?)?;

    module.add_class::<VarRequirementsValidator>()?;
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
