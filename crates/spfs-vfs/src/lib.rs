// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Virtual Filesystem Implementations for SPFS
//!
//! Notably, provides the logic to run spfs over FUSE on linux
//! and winfsp on windows.

#![deny(missing_docs)]

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
