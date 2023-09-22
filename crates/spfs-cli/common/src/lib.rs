#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

//! Common macros and argument structures for the spfs command line

mod args;

pub mod __private {
    // Private re-exports for macros
    pub use {libc, spfs};
}

pub use args::{capture_if_relevant, CommandName, Logging, Render, Sync};
#[cfg(feature = "sentry")]
pub use args::{configure_sentry, shutdown_sentry};
