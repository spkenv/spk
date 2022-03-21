// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::fixture;

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

#[fixture]
pub fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spk-test-").expect("Failed to establish temporary directory for testing")
}

#[fixture]
pub fn tmprepo() -> crate::storage::RepositoryHandle {
    crate::storage::RepositoryHandle::Mem(Default::default())
}
