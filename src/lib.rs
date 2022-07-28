// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]

pub mod api;
pub mod build;
mod env;
mod error;
pub mod exec;
mod global;
pub mod io;
pub mod parsing;
mod publish;
pub mod solve;
pub mod storage;
pub mod test;

#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(feature = "test-macros")]
pub mod macros;

pub use env::current_env;
pub use error::{Error, Result};
pub use exec::{build_required_packages, setup_current_runtime, setup_runtime};
pub use global::{load_spec, save_spec};
pub use publish::Publisher;
pub use solve::{Solution, Solver};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

lazy_static::lazy_static! {
    pub static ref HANDLE: tokio::runtime::Handle = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = rt.handle().clone();
        std::thread::spawn(move || rt.block_on(futures::future::pending::<()>()));
        handle
    };
}

#[async_trait::async_trait]
pub trait ResolverCallback: Send + Sync {
    /// Run a solve using the given [`solve::SolverRuntime`],
    /// producing a [`crate::Solution`].
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a mut solve::SolverRuntime,
    ) -> Result<crate::Solution>;
}

/// A no-frills implementation of [`ResolverCallback`].
struct DefaultResolver {}

#[async_trait::async_trait]
impl ResolverCallback for DefaultResolver {
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a mut solve::SolverRuntime,
    ) -> Result<crate::Solution> {
        r.solution().await
    }
}

type BoxedResolverCallback<'a> = Box<dyn ResolverCallback + 'a>;
