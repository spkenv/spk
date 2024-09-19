// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Common macros and argument structures for the spfs command line

mod args;

pub mod __private {
    // Private re-exports for macros
    pub use {libc, spfs};
}

pub use args::{
    capture_if_relevant,
    AnnotationViewing,
    CommandName,
    Logging,
    Progress,
    Render,
    Sync,
};
#[cfg(feature = "sentry")]
pub use args::{configure_sentry, shutdown_sentry};
