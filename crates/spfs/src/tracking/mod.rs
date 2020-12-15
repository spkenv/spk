///! Object tracking and definitions
mod diff;
pub use diff::{compute_diff, Diff, DiffMode};
mod entry;
pub use entry::{Entry, EntryKind};
mod env;
pub use env::{parse_env_spec, EnvSpec};
mod manifest;
pub use manifest::{compute_manifest, Manifest, ManifestBuilder};
mod object;
pub use object::Object;
mod tag;
pub use tag::{build_tag_spec, split_tag_spec, Tag, TagSpec};
