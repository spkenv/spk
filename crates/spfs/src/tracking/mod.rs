// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Object tracking and definitions

pub mod blob_reader;
mod diff;
mod entry;
mod env;
pub mod manifest;
mod object;
mod tag;

pub use blob_reader::{BlobRead, BlobReadExt};
pub use diff::{Diff, DiffMode, compute_diff};
pub use entry::{Entry, EntryKind};
pub use env::{
    ENV_SPEC_EMPTY, ENV_SPEC_SEPARATOR, EnvSpec, EnvSpecItem, SpecFile, clear_seen_spec_file_cache,
};
pub use manifest::{
    BlobHasher, ComputeManifestReporter, DEFAULT_MAX_CONCURRENT_BLOBS,
    DEFAULT_MAX_CONCURRENT_BRANCHES, Manifest, ManifestBuilder, ManifestNode, OwnedManifestNode,
    PathFilter, compute_manifest,
};
pub use object::Object;
pub use tag::{Tag, TagSpec, build_tag_spec, split_tag_spec};
mod time_spec;
pub use time_spec::{TimeSpec, parse_duration, parse_time};
