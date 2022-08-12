// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Protocol Buffer message formats and conversions.

mod conversions;
mod result;
mod generated {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("spfs");
}

pub use conversions::*;
pub use generated::*;
pub(crate) use result::RpcResult;

#[cfg(feature = "server")]
pub(crate) use result::handle_error;
