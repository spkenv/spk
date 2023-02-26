// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Virtual Filesystem Implementations for SPFS
//!
//! Notably, provides the logic to run spfs over FUSE.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod fuse;

pub use fuse::{Config, Session};
