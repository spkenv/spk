// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Filesystem isolation, capture and distribution.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures;

pub mod bootstrap;
pub mod check;
pub mod clean;
pub mod commit;
pub mod config;
mod diff;
#[cfg_attr(windows, path = "./env_win.rs")]
pub mod env;
mod error;
pub mod find_path;
pub mod graph;
pub mod io;
#[cfg_attr(windows, path = "./monitor_win.rs")]
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
    Shell,
    ShellKind,
    build_command_for_runtime,
    build_interactive_shell_command,
    build_shell_initialized_command,
};
pub use check::Checker;
pub use clean::Cleaner;
pub use commit::Committer;
pub use diff::{diff, diff_runtime_changes, runtime_active_changes};
pub use encoding::Digest;
pub use error::{Error, OsError, OsErrorExt, Result, SyncError, SyncResult};
pub use resolve::{
    RenderResult,
    compute_environment_manifest,
    compute_manifest,
    compute_object_manifest,
    resolve_stack_to_layers,
    resolve_stack_to_layers_with_repo,
    which,
    which_spfs,
};
pub use spfs_encoding as encoding;
pub use status::{
    active_runtime,
    change_to_durable_runtime,
    compute_runtime_manifest,
    exit_runtime,
    get_runtime_backing_repo,
    initialize_runtime,
    make_active_runtime_editable,
    make_runtime_durable,
    reinitialize_runtime,
    remount_runtime,
};
pub use sync::Syncer;

pub use self::config::{
    Config,
    RemoteAddress,
    RemoteConfig,
    Sentry,
    get_config,
    load_config,
    open_repository,
};
