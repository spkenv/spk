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
pub use diff::{compute_diff, Diff, DiffMode};
pub use entry::{Entry, EntryKind};
pub use env::{
    clear_seen_spec_file_cache,
    EnvSpec,
    EnvSpecItem,
    SpecFile,
    ENV_SPEC_EMPTY,
    ENV_SPEC_SEPARATOR,
};
pub use manifest::{
    compute_manifest,
    BlobHasher,
    ComputeManifestReporter,
    Manifest,
    ManifestBuilder,
    ManifestNode,
    OwnedManifestNode,
    PathFilter,
    DEFAULT_MAX_CONCURRENT_BLOBS,
    DEFAULT_MAX_CONCURRENT_BRANCHES,
};
pub use object::Object;
pub use tag::{build_tag_spec, split_tag_spec, Tag, TagSpec};
mod time_spec;
pub use time_spec::{parse_duration, parse_time, TimeSpec};
