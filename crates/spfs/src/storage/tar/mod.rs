// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! An spfs storage implementation where all data is unpacked and repacked
//! into a tar archive on disk

mod repository;
pub use repository::{Config, TarRepository};
