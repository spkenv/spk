// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Filesystem isolation, capture and distribution.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
pub mod fixtures;

mod bootstrap;
mod clean;
mod commit;
pub mod config;
mod diff;
pub mod encoding;
pub mod env;
mod error;
pub mod graph;
pub mod io;
mod ls_tags;
pub mod prelude;
pub mod proto;
mod prune;
mod resolve;
pub mod runtime;
#[cfg(feature = "server")]
pub mod server;
mod status;
pub mod storage;
mod sync;
pub mod tracking;

// re-exported to make downstream implementations easier
pub use async_trait::async_trait;

pub use self::config::{
    get_config, load_config, open_repository, Config, RemoteAddress, RemoteConfig,
};
pub use bootstrap::{
    build_command_for_runtime, build_interactive_shell_command, build_shell_initialized_command,
};
pub use clean::{
    clean_untagged_objects, get_all_attached_objects, get_all_unattached_objects,
    get_all_unattached_payloads, purge_objects,
};
pub use commit::{commit_dir, commit_layer, commit_platform};
pub use diff::diff;
pub use encoding::Digest;
pub use error::{Error, Result};
pub use ls_tags::ls_tags;
pub use prune::{get_prunable_tags, prune_tags, PruneParameters};
pub use resolve::{
    compute_manifest, compute_object_manifest, render, render_into_directory,
    resolve_stack_to_layers, which, which_spfs,
};
pub use status::{
    active_runtime, compute_runtime_manifest, initialize_runtime, make_active_runtime_editable,
    reinitialize_runtime, remount_runtime,
};
pub use sync::Syncer;
