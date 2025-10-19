// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Protocol Buffer message formats and conversions.

mod conversions;
mod result;
mod generated {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("spfs");
}

#[cfg(feature = "server")]
pub(crate) use conversions::convert_digest;
pub(crate) use conversions::{convert_from_datetime, convert_payload_digest};
pub use generated::*;
pub(crate) use result::RpcResult;
#[cfg(feature = "server")]
pub(crate) use {conversions::convert_to_datetime, result::handle_error};
