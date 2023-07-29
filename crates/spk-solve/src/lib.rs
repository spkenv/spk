// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod io;
#[cfg(feature = "statsd")]
mod metrics;
mod search_space;
mod solver;
mod status_line;

use std::sync::Arc;

pub use error::{Error, Result};
use graph::Graph;
pub use io::{DecisionFormatter, DecisionFormatterBuilder, MultiSolverKind};
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
    /// Run a solve using the given [`crate::Solver`],
    /// producing a [`crate::Solution`].
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a Solver,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)>;
}

/// A no-frills implementation of [`ResolverCallback`].
pub struct DefaultResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultResolver {
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a Solver,
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
