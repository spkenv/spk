// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod io;
#[cfg(feature = "statsd")]
mod metrics;
mod search_space;
mod solvers;
mod status_line;

use std::sync::Arc;

pub use error::{Error, Result};
use graph::Graph;
pub use io::{
    DEFAULT_SOLVER_RUN_FILE_PREFIX,
    DecisionFormatter,
    DecisionFormatterBuilder,
    MultiSolverKind,
};
#[cfg(feature = "statsd")]
pub use metrics::{
    MetricsClient,
    SPK_ERROR_COUNT_METRIC,
    SPK_RUN_COUNT_METRIC,
    SPK_RUN_TIME_METRIC,
    SPK_SOLUTION_PACKAGE_COUNT_METRIC,
    SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC,
    SPK_SOLVER_RUN_COUNT_METRIC,
    SPK_SOLVER_RUN_TIME_METRIC,
    SPK_SOLVER_SOLUTION_SIZE_METRIC,
    get_metrics_client,
};
pub(crate) use search_space::show_search_space_stats;
pub use solvers::{StepSolver, StepSolverRuntime};
pub use spk_schema::foundation::ident_build::Build;
pub use spk_schema::foundation::ident_component::Component;
pub use spk_schema::foundation::option_map;
pub use spk_schema::foundation::spec_ops::{Named, Versioned};
pub use spk_schema::ident::{
    AnyIdent,
    BuildIdent,
    PkgRequest,
    Request,
    RequestedBy,
    parse_ident_range,
};
pub use spk_schema::{Package, Recipe, Spec, SpecRecipe, recipe, spec, v0};
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
    /// Run a solve using the given [`crate::StepSolver`],
    /// producing a [`crate::Solution`].
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a StepSolver,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)>;
}

/// A no-frills implementation of [`ResolverCallback`].
pub struct DefaultResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultResolver {
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a StepSolver,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        let mut runtime = r.run();
        let solution = runtime.solution().await;
        match solution {
            Err(err) => Err(err),
            Ok(s) => Ok((s, runtime.graph())),
        }
    }
}

pub type BoxedResolverCallback<'a> = Box<dyn ResolverCallback + 'a>;
