// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Uses a local directory on disk to store the spfs repository.

mod database;
mod hash_store;
mod manifest_render_path;
mod payloads;
mod render_summary;
mod renderer;
mod repository;
mod tag;

pub mod migrations;
mod render_reporter;

pub use hash_store::FsHashStore;
pub use manifest_render_path::ManifestRenderPath;
pub use render_reporter::{
    ConsoleRenderReporter,
    MultiReporter,
    RenderReporter,
    SilentRenderReporter,
};
pub use render_summary::{RenderSummary, RenderSummaryReporter};
pub use renderer::{
    CliRenderType,
    DEFAULT_MAX_CONCURRENT_BLOBS,
    DEFAULT_MAX_CONCURRENT_BRANCHES,
    HardLinkRenderType,
    RenderType,
    Renderer,
};
#[cfg(any(test, feature = "test-fixtures"))]
pub use repository::MaybeOpenFsRepositoryImpl;
pub use repository::{
    Config,
    DURABLE_EDITS_DIR,
    FsRepositoryOps,
    MaybeOpenFsRepository,
    OpenFsRepository,
    Params,
    RenderStore,
    read_last_migration_version,
};
