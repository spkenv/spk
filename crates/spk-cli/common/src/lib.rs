// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]

mod cli;
mod env;
mod error;
pub mod exec;
pub mod flags;
mod publish;

pub use cli::{CommandArgs, Run};
pub use env::{configure_logging, current_env, spk_exe};
pub use error::{Error, Result, TestError};
pub use exec::build_required_packages;
pub use publish::Publisher;

#[cfg(feature = "sentry")]
pub use env::configure_sentry;

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
