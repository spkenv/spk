// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod abstract_solver;
pub mod cdcl_solver;
mod error;
mod io;
#[cfg(feature = "statsd")]
mod metrics;
mod search_space;
mod solver;
mod status_line;

use std::sync::Arc;

pub use abstract_solver::AbstractSolver;
// Publicly exported CdclSolver to stop dead code warnings
pub use cdcl_solver::Solver as CdclSolver;
pub use error::{Error, Result};
use graph::Graph;
pub use io::{
    DecisionFormatter,
    DecisionFormatterBuilder,
    MultiSolverKind,
    DEFAULT_SOLVER_RUN_FILE_PREFIX,
};
#[cfg(feature = "statsd")]
pub use metrics::{
    get_metrics_client,
    MetricsClient,
    SPK_ERROR_COUNT_METRIC,
    SPK_RUN_COUNT_METRIC,
    SPK_RUN_TIME_METRIC,
    SPK_SOLUTION_PACKAGE_COUNT_METRIC,
    SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC,
    SPK_SOLVER_RUN_COUNT_METRIC,
    SPK_SOLVER_RUN_TIME_METRIC,
    SPK_SOLVER_SOLUTION_SIZE_METRIC,
};
pub(crate) use search_space::show_search_space_stats;
pub use solver::{Solver, SolverRuntime};
pub use spk_schema::foundation::ident_build::Build;
pub use spk_schema::foundation::ident_component::Component;
pub use spk_schema::foundation::option_map;
pub use spk_schema::foundation::spec_ops::{Named, Versioned};
pub use spk_schema::ident::{
    parse_ident_range,
    AnyIdent,
    BuildIdent,
    PkgRequest,
    Request,
    RequestedBy,
};
pub use spk_schema::{recipe, spec, v0, Package, Recipe, Spec, SpecRecipe};
pub use spk_solve_solution::{PackageSource, Solution};
pub use spk_storage::RepositoryHandle;
pub(crate) use status_line::StatusLine;
pub use {
    serde_json,
    spfs,
    spk_solve_graph as graph,
    spk_solve_package_iterator as package_iterator,
    spk_solve_solution as solution,
    spk_solve_validation as validation,
};

#[async_trait::async_trait]
pub trait ResolverCallback: Send + Sync {
    type Solver;
    type SolveResult;

    /// Run a solve using the given [`Self::Solver`] producing a [`Self::SolveResult`].
    async fn solve<'s, 'a: 's>(&'s self, r: &'a Self::Solver) -> Result<Self::SolveResult>;
}

/// A no-frills implementation of [`ResolverCallback`].
pub struct DefaultResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultResolver {
    type Solver = Solver;
    type SolveResult = (Solution, Arc<tokio::sync::RwLock<Graph>>);

    async fn solve<'s, 'a: 's>(&'s self, r: &'a Self::Solver) -> Result<Self::SolveResult> {
        let mut runtime = r.run();
        let solution = runtime.solution().await;
        match solution {
            Err(err) => Err(err),
            Ok(s) => Ok((s, runtime.graph())),
        }
    }
}

pub type BoxedResolverCallback<'a> = Box<
    dyn ResolverCallback<Solver = Solver, SolveResult = (Solution, Arc<tokio::sync::RwLock<Graph>>)>
        + 'a,
>;

/// Another no-frills implementation of [`ResolverCallback`].
pub struct DefaultCdclResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultCdclResolver {
    type Solver = cdcl_solver::Solver;
    type SolveResult = Solution;

    async fn solve<'s, 'a: 's>(&'s self, r: &'a Self::Solver) -> Result<Self::SolveResult> {
        r.solve().await
    }
}

pub type BoxedCdclResolverCallback<'a> =
    Box<dyn ResolverCallback<Solver = cdcl_solver::Solver, SolveResult = Solution> + 'a>;
