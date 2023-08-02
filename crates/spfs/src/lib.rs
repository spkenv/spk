// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

//! Filesystem isolation, capture and distribution.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
pub mod fixtures;

mod bootstrap;
pub mod check;
pub mod clean;
pub mod commit;
pub mod config;
mod diff;
pub mod env;
mod error;
pub mod graph;
pub mod io;
pub mod monitor;
pub mod prelude;
pub mod proto;
mod prune;
mod repeating_timeout;
mod resolve;
pub mod runtime;
#[cfg(feature = "server")]
pub mod server;
mod status;
pub mod storage;
pub mod sync;
pub mod tracking;

// re-exported to make downstream implementations easier
pub use async_trait::async_trait;
pub use bootstrap::{
    build_command_for_runtime,
    build_interactive_shell_command,
    build_shell_initialized_command,
    Shell,
    ShellKind,
};
pub use check::Checker;
pub use clean::Cleaner;
pub use commit::Committer;
pub use diff::{diff, diff_runtime_changes};
pub use encoding::Digest;
pub use error::{Error, Result};
pub use resolve::{
    compute_environment_manifest,
    compute_manifest,
    compute_object_manifest,
    resolve_stack_to_layers,
    resolve_stack_to_layers_with_repo,
    which,
    which_spfs,
    RenderResult,
};
pub use spfs_encoding as encoding;
pub use status::{
    active_runtime,
    compute_runtime_manifest,
    exit_runtime,
    get_runtime_backing_repo,
    initialize_runtime,
    make_active_runtime_editable,
    reinitialize_runtime,
    remount_runtime,
};
pub use sync::Syncer;

pub use self::config::{
    get_config,
    load_config,
    open_repository,
    Config,
    RemoteAddress,
    RemoteConfig,
};
