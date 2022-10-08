// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod format;
mod ident;
pub mod parsing;
mod range_ident;
mod request;

pub use error::{Error, Result};
pub use ident::{parse_ident, BuildIdent, Ident};
pub use range_ident::{parse_ident_range, RangeIdent};
pub use request::{
    is_false,
    InclusionPolicy,
    NameAndValue,
    PkgRequest,
    PreReleasePolicy,
    Request,
    RequestedBy,
    VarRequest,
};
