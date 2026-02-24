// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Common macros and argument structures for the spfs command line.
//!
//! This crate provides shared CLI argument structures, logging configuration,
//! and utility macros used across spfs CLI commands.

mod args;

#[doc(hidden)]
pub mod __private {
    pub use {libc, spfs};
}

pub use args::{
    AnnotationViewing,
    CommandName,
    HasRepositoryArgs,
    Logging,
    Progress,
    Render,
    Repositories,
    Sync,
    capture_if_relevant,
};
#[cfg(feature = "sentry")]
pub use args::{configure_sentry, shutdown_sentry};
