// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Protocol Buffer message formats and conversions for the spfs virtual filesystem.

mod generated {
    #![allow(missing_docs)]
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("spfs_vfs");
}

pub use generated::*;
