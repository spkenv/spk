// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
mod format;
mod ident;
pub mod parsing;
mod request;

pub use error::{Error, Result};
pub use ident::{parse_ident, BuildIdent, Ident};
pub use request::{
    is_false, parse_ident_range, InclusionPolicy, PkgRequest, PreReleasePolicy, RangeIdent,
    Request, RequestedBy, VarRequest, KNOWN_REPOSITORY_NAMES,
};
