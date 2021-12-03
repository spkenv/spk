// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Filesystem isolation, capture and distribution.

#[macro_use]
extern crate serde_derive;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
#[macro_use]
pub mod fixtures;

pub mod encoding;
pub mod env;
pub mod graph;
pub mod io;
pub mod prelude;
pub mod runtime;
pub mod storage;
pub mod tracking;

mod error;
pub use error::{Error, Result};

mod config;
pub use self::config::{load_config, Config};
mod resolve;
pub use resolve::{
    compute_manifest, compute_object_manifest, render, render_into_directory,
    resolve_stack_to_layers, which, which_spfs,
};
mod status;
pub use status::{
    active_runtime, compute_runtime_manifest, initialize_runtime, make_active_runtime_editable,
    reinitialize_runtime, remount_runtime,
};
mod bootstrap;
pub use bootstrap::{
    build_command_for_runtime, build_interactive_shell_cmd, build_shell_initialized_command,
};
mod sync;
pub use sync::{pull_ref, push_ref, sync_object, sync_ref};
mod commit;
pub use commit::{commit_layer, commit_platform};
mod clean;
pub use clean::{
    clean_untagged_objects, get_all_attached_objects, get_all_unattached_objects,
    get_all_unattached_payloads, purge_objects,
};
mod prune;
pub use prune::{get_prunable_tags, prune_tags, PruneParameters};
mod diff;
pub use diff::diff;
mod ls_tags;
pub use ls_tags::ls_tags;
