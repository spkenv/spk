// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Uses a local directory on disk to store the spfs repository.

mod database;
mod hash_store;
mod payloads;
mod render_summary;
mod renderer;
mod repository;
mod tag;

pub mod migrations;
mod render_reporter;

pub use hash_store::FSHashStore;
pub use render_reporter::{
    ConsoleRenderReporter,
    MultiReporter,
    RenderReporter,
    SilentRenderReporter,
};
pub use render_summary::{RenderSummary, RenderSummaryReporter};
pub use renderer::{
    RenderType,
    Renderer,
    DEFAULT_MAX_CONCURRENT_BLOBS,
    DEFAULT_MAX_CONCURRENT_BRANCHES,
};
pub use repository::{read_last_migration_version, Config, FSRepository, RenderStore};
