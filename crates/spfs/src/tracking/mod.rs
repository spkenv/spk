// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

///! Object tracking and definitions
pub mod blob_reader;
mod diff;
mod entry;
mod env;
mod manifest;
mod object;
mod tag;

pub use blob_reader::{BlobRead, BlobReadExt};
pub use diff::{compute_diff, Diff, DiffMode};
pub use entry::{Entry, EntryKind};
pub use env::{EnvSpec, EnvSpecItem, ENV_SPEC_SEPARATOR};
pub use manifest::{
    compute_manifest,
    BlobHasher,
    Manifest,
    ManifestBuilder,
    ManifestNode,
    PathFilter,
};
pub use object::Object;
pub use tag::{build_tag_spec, split_tag_spec, Tag, TagSpec};
