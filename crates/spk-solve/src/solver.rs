// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use spk_schema::{OptionMap, Request};
use spk_solve_solution::Solution;
use spk_storage::RepositoryHandle;
use variantly::Variantly;

use crate::Result;

#[enum_dispatch(Solver)]
#[derive(Variantly)]
pub(crate) enum SolverImpl {
    Step(crate::StepSolver),
    Resolvo(crate::solvers::ResolvoSolver),
}

#[async_trait::async_trait]
#[enum_dispatch]
pub trait Solver {
    /// Add a repository where the solver can get packages.
    fn add_repository<R>(&mut self, repo: R)
    where
        R: Into<Arc<RepositoryHandle>>;

    /// Add a request to this solver.
    fn add_request(&mut self, request: Request);

    /// Put this solver back into its default state
    fn reset(&mut self);

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
