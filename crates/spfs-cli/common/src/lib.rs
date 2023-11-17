// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Common macros and argument structures for the spfs command line

mod args;

pub mod __private {
    // Private re-exports for macros
    pub use {libc, spfs};
}

pub use args::{capture_if_relevant, CommandName, Logging, Render, Sync};
#[cfg(feature = "sentry")]
pub use args::{configure_sentry, shutdown_sentry};
