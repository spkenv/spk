// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Virtual Filesystem Implementations for SPFS
//!
//! Notably, provides the logic to run spfs over FUSE on linux
//! and winfsp on windows.

#![deny(missing_docs)]

#[featurecomb::comb]
mod _featurecomb {}

mod error;
pub use error::Error;

#[cfg(all(unix, feature = "fuse-backend"))]
mod fuse;
#[cfg(all(windows, feature = "winfsp-backend"))]
pub mod proto;
#[cfg(all(windows, feature = "winfsp-backend"))]
pub mod winfsp;

#[cfg(all(unix, feature = "fuse-backend"))]
pub use fuse::{Config, Session};
#[cfg(all(windows, feature = "winfsp-backend"))]
pub use winfsp::{Config, Service};
