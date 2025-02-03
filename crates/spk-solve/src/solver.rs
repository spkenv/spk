// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use spk_schema::{OptionMap, Request};
use spk_storage::RepositoryHandle;
use variantly::Variantly;

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

    fn update_options(&mut self, options: OptionMap);
}
