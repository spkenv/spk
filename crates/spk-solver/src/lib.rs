// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod io;
mod macros;
mod solver;

// Re-export for macros
pub use serde_json;
pub use spfs;
pub use spk_foundation::option_map;
pub use spk_foundation::spec_ops::{Named, PackageOps, RecipeOps, Versioned};
pub use spk_ident::{parse_ident_range, PkgRequest, Request, RequestedBy};
pub use spk_ident_component::Component;
pub use spk_solver_solution::{PackageSource, Solution};
pub use spk_spec::{recipe, spec, v0, Package, Recipe, Spec};
pub use spk_storage::RepositoryHandle;

pub use error::{Error, Result};
pub use io::{DecisionFormatter, DecisionFormatterBuilder};
pub use solver::{Solver, SolverRuntime};

#[async_trait::async_trait]
pub trait ResolverCallback: Send + Sync {
    /// Run a solve using the given [`crate::SolverRuntime`],
    /// producing a [`crate::Solution`].
    async fn solve<'s, 'a: 's>(&'s self, r: &'a mut SolverRuntime) -> Result<Solution>;
}

/// A no-frills implementation of [`ResolverCallback`].
pub struct DefaultResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultResolver {
    async fn solve<'s, 'a: 's>(&'s self, r: &'a mut SolverRuntime) -> Result<Solution> {
        r.solution().await
    }
}

pub type BoxedResolverCallback<'a> = Box<dyn ResolverCallback + 'a>;
