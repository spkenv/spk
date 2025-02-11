// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use spk_schema::ident::{PkgRequest, VarRequest};
use spk_schema::{OptionMap, Request};
use spk_solve_solution::Solution;
use spk_storage::RepositoryHandle;
use variantly::Variantly;

use crate::{DecisionFormatter, Result};

#[enum_dispatch(AbstractSolver, AbstractSolverExt, AbstractSolverMut)]
#[derive(Clone, Variantly)]
pub enum SolverImpl {
    Og(crate::Solver),
    Cdcl(crate::cdcl_solver::Solver),
}

#[async_trait::async_trait]
#[enum_dispatch]
pub trait AbstractSolver {
    fn as_any(&self) -> &dyn std::any::Any;

    /// Return the PkgRequests added to the solver.
    fn get_pkg_requests(&self) -> Vec<PkgRequest>;

    /// Return the VarRequests added to the solver.
    fn get_var_requests(&self) -> Vec<VarRequest>;

    /// Return a reference to the solver's list of repositories.
    fn repositories(&self) -> &[Arc<RepositoryHandle>];
}

#[async_trait::async_trait]
#[enum_dispatch]
pub trait AbstractSolverMut {
    /// Add a request to this solver.
    fn add_request(&mut self, request: Request);

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

impl<T> AbstractSolver for &T
where
    T: AbstractSolver,
{
    fn as_any(&self) -> &dyn std::any::Any {
        T::as_any(self)
    }

    fn get_pkg_requests(&self) -> Vec<PkgRequest> {
        T::get_pkg_requests(self)
    }

    fn get_var_requests(&self) -> Vec<VarRequest> {
        T::get_var_requests(self)
    }

    fn repositories(&self) -> &[Arc<RepositoryHandle>] {
        T::repositories(self)
    }
}

impl<T> AbstractSolver for &mut T
where
    T: AbstractSolver,
{
    fn as_any(&self) -> &dyn std::any::Any {
        T::as_any(self)
    }

    fn get_pkg_requests(&self) -> Vec<PkgRequest> {
        T::get_pkg_requests(self)
    }

    fn get_var_requests(&self) -> Vec<VarRequest> {
        T::get_var_requests(self)
    }

    fn repositories(&self) -> &[Arc<RepositoryHandle>] {
        T::repositories(self)
    }
}

#[async_trait::async_trait]
impl<T> AbstractSolverMut for &mut T
where
    T: AbstractSolverMut + Send + Sync,
{
    fn add_request(&mut self, request: Request) {
        T::add_request(self, request)
    }

    fn reset(&mut self) {
        T::reset(self)
    }

    async fn run_and_log_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        T::run_and_log_resolve(self, formatter).await
    }

    async fn run_and_print_resolve(&mut self, formatter: &DecisionFormatter) -> Result<Solution> {
        T::run_and_print_resolve(self, formatter).await
    }

    fn set_binary_only(&mut self, binary_only: bool) {
        T::set_binary_only(self, binary_only)
    }

    async fn solve(&mut self) -> Result<Solution> {
        T::solve(self).await
    }

    fn update_options(&mut self, options: OptionMap) {
        T::update_options(self, options)
    }
}

#[async_trait::async_trait]
#[enum_dispatch]
pub trait AbstractSolverExt: AbstractSolver {
    /// Add a repository where the solver can get packages.
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>;
}

impl<T> AbstractSolverExt for &mut T
where
    T: AbstractSolverExt + Sync,
{
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>,
    {
        T::add_repository(self, repo);
    }
}
