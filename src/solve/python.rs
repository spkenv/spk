// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use pyo3::py_run;
use pyo3::wrap_pyfunction;
use std::sync::Arc;

use crate::api;
use crate::build::BuildVariant;

use super::errors::SolverError;
use super::graph;
use super::graph::{
    Decision, Graph, Node, Note, RequestPackage, RequestVar, SetOptions, SetPackage,
    SetPackageBuild, SkipPackageNote, StepBack,
};
use super::solution::Solution;
use super::solver::{Solver, SolverFailedError};
use super::validation::{self, Validators, VarRequirementsValidator};
use super::PackageSource;

/// A single change made to a state.
#[pyclass(subclass)]
#[derive(Clone)]
pub struct Change {}

#[pyclass]
#[derive(Clone, Debug)]
pub struct State {
    state: Arc<graph::State>,
}

impl<'a> From<&'a State> for &'a graph::State {
    fn from(o: &'a State) -> Self {
        &*o.state
    }
}

impl<'a> From<&'a State> for &'a Arc<graph::State> {
    fn from(o: &'a State) -> Self {
        &o.state
    }
}

impl From<graph::State> for State {
    fn from(o: graph::State) -> Self {
        State { state: Arc::new(o) }
    }
}

impl From<Arc<graph::State>> for State {
    fn from(o: Arc<graph::State>) -> Self {
        State {
            state: Arc::clone(&o),
        }
    }
}

#[pymethods]
impl State {
    #[new]
    pub fn newpy(
        pkg_requests: Vec<api::PkgRequest>,
        var_requests: Vec<api::VarRequest>,
        options: Vec<(String, String)>,
        packages: Vec<(api::Spec, PackageSource)>,
        #[allow(unused_variables)] hash_cache: Vec<u64>,
    ) -> Self {
        graph::State::new(
            pkg_requests,
            var_requests,
            packages
                .into_iter()
                .map(|(s, ps)| {
                    (
                        Arc::new(api::SpecWithBuildVariant {
                            spec: Arc::new(s),
                            // XXX: hard coded variant here. This method will go away soon?
                            variant: BuildVariant::Default,
                        }),
                        ps,
                    )
                })
                .collect(),
            options,
        )
        .into()
    }

    #[staticmethod]
    pub fn default() -> Self {
        graph::State::default().into()
    }

    pub fn get_option_map(&self) -> api::OptionMap {
        self.state.get_option_map()
    }

    #[getter]
    pub fn id(&self) -> u64 {
        self.state.id()
    }

    #[getter]
    pub fn pkg_requests(&self) -> Vec<api::PkgRequest> {
        (*self.state.pkg_requests).clone()
    }
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
    module.add_class::<State>()?;
    module.add_class::<StepBack>()?;
    Ok(())
}

#[pyfunction]
fn default_validators() -> Vec<Validators> {
    validation::default_validators().into()
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
    add_submodule!(m, py, "graph", init_submodule_graph);
    add_submodule!(m, py, "validation", init_submodule_validation);

    m.add_class::<Graph>()?;
    m.add_class::<Solution>()?;
    m.add_class::<Solver>()?;

    m.add("SolverError", py.get_type::<SolverError>())?;
    m.add("SolverFailedError", py.get_type::<SolverFailedError>())?;

    Ok(())
}
