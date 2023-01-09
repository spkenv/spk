//! Common macros and argument structures for the spfs command line

mod args;

pub mod __private {
    // Private re-exports for macros
    pub use libc;
}

#[cfg(feature = "sentry")]
pub use args::configure_sentry;
pub use args::{capture_if_relevant, configure_logging, configure_spops, Sync};
