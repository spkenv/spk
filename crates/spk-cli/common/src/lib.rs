// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod cli;
mod env;
mod error;
pub mod exec;
pub mod flags;
pub mod parsing;
mod publish;
pub mod with_version_and_build_set;

pub use cli::{CommandArgs, Run};
#[cfg(feature = "sentry")]
pub use env::configure_sentry;
pub use env::{configure_logging, current_env, spk_exe};
pub use error::{Error, Result, TestError};
pub use exec::build_required_packages;
use once_cell::sync::Lazy;
pub use publish::Publisher;
pub use with_version_and_build_set::{DefaultBuildStrategy, DefaultVersionStrategy};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub static HANDLE: Lazy<tokio::runtime::Handle> = Lazy::new(|| {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();
    std::thread::spawn(move || rt.block_on(futures::future::pending::<()>()));
    handle
});
