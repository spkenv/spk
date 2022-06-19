// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
pub mod api;
pub mod build;
mod env;
mod error;
pub mod exec;
mod global;
pub mod io;
mod publish;
pub mod solve;
pub mod storage;
pub mod test;

#[cfg(test)]
mod fixtures;
#[cfg(feature = "test-macros")]
pub mod macros;

pub use env::current_env;
pub use error::{CloneableResult, Error, Result};
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
