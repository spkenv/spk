// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Protocol Buffer message formats and conversions.

mod conversions;
mod result;
mod generated {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("spfs");
}

pub(crate) use conversions::{convert_digest, convert_from_datetime};
pub use generated::*;
pub(crate) use result::RpcResult;

#[cfg(feature = "server")]
pub(crate) use {conversions::convert_to_datetime, result::handle_error};
