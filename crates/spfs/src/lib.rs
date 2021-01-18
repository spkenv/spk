//! Filesystem isolation, capture and distribution.

#[macro_use]
extern crate serde_derive;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub mod encoding;
pub mod graph;
pub mod io;
pub mod prelude;
pub mod runtime;
pub mod storage;
pub mod tracking;

mod error;
pub use error::{Error, Result};

mod config;
pub use self::config::{get_config, load_config, Config};
mod resolve;
pub use resolve::{compute_manifest, compute_object_manifest};
mod status;
pub use status::{
    active_runtime, compute_runtime_manifest, deinitialize_runtime, initialize_runtime,
    make_active_runtime_editable, remount_runtime, NoRuntimeError,
};
mod bootstrap;
pub use bootstrap::{
    build_command_for_runtime, build_interactive_shell_cmd, build_shell_initialized_command,
};
mod sync;
pub use sync::{pull_ref, push_ref, sync_ref};
mod commit;
pub use commit::{commit_layer, commit_platform, NothingToCommitError};
mod clean;
pub use clean::{
    clean_untagged_objects, get_all_attached_objects, get_all_unattached_objects, purge_objects,
};
mod prune;
pub use prune::{get_prunable_tags, prune_tags, PruneParameters};
mod diff;
pub use diff::diff;
mod ls_tags;
pub use ls_tags::ls_tags;
