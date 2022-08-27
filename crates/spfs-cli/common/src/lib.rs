//! Common macros and argument structures for the spfs command line

mod args;

#[cfg(feature = "sentry")]
pub use args::configure_sentry;
pub use args::{capture_if_relevant, configure_logging, configure_spops, Sync};
