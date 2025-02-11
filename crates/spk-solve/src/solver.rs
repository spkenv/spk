// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::any::Any;
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use spk_schema::ident::{PkgRequest, VarRequest};
use spk_schema::{OptionMap, Request};
use spk_solve_solution::Solution;
use spk_storage::RepositoryHandle;
use variantly::Variantly;

use crate::{DecisionFormatter, Result};

#[enum_dispatch(Solver)]
#[derive(Variantly)]
pub(crate) enum SolverImpl {
    Step(crate::StepSolver),
    Resolvo(crate::solvers::ResolvoSolver),
}

#[async_trait::async_trait]
#[enum_dispatch]
pub trait Solver: Any {
    /// Add a repository where the solver can get packages.
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>;

    /// Add a request to this solver.
    fn add_request(&mut self, request: Request);

    /// Return the PkgRequests added to the solver.
    fn get_pkg_requests(&self) -> Vec<PkgRequest>;

    /// Return the VarRequests added to the solver.
    fn get_var_requests(&self) -> Vec<VarRequest>;

    /// Return a reference to the solver's list of repositories.
    fn repositories(&self) -> &[Arc<RepositoryHandle>];

    /// Put this solver back into its default state
    fn reset(&mut self);

    /// Run the solver as configured using the given formatter.
    ///
    /// "log" means that solver progress is output via tracing, as
    /// configured by the formatter.
    ///
    /// The solution may also be printed, if found, as configured by the
    /// formatter.
    ///
    /// Not all formatter settings may be supported by every solver.
    async fn run_and_log_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution>;

    /// Run the solver as configured using the given formatter.
    ///
    /// "print" means that solver progress is printed to the console, as
    /// configured by the formatter.
    ///
    /// The solution may also be printed, if found, as configured by the
    /// formatter.
    ///
    /// Not all formatter settings may be supported by every solver.
    async fn run_and_print_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution>;

    /// If true, only solve pre-built binary packages.
    ///
    /// When false, the solver may return packages where the build is not set.
    /// These packages are known to have a source package available, and the requested
    /// options are valid for a new build of that source package.
    /// These packages are not actually built as part of the solver process but their
    /// build environments are fully resolved and dependencies included
    fn set_binary_only(&mut self, binary_only: bool);

    /// Run the solver as configured.
    async fn solve(&mut self) -> Result<Solution>;

    fn update_options(&mut self, options: OptionMap);
}
