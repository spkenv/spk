// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use once_cell::sync::Lazy;
use rstest::fixture;
use tracing_capture::{CaptureLayer, SharedStorage};
use tracing_subscriber::prelude::*;

static TRACING_STORAGE: Lazy<SharedStorage> = Lazy::new(SharedStorage::default);

/// Initialize tracing logs for testing.
///
/// Returns a shared storage instance for checking
/// events and spans that were logged
pub fn init_logging() -> &'static SharedStorage {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_test_writer()
        .finish()
        .with(CaptureLayer::new(&TRACING_STORAGE));
    let _ = tracing::subscriber::set_global_default(sub);
    &TRACING_STORAGE
}

#[fixture]
pub fn tmpdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("spk-test-")
        .tempdir()
        .expect("Failed to establish temporary directory for testing")
}
