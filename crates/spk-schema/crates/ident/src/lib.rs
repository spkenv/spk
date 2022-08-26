// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod format;
mod ident;
mod ident_any;
mod ident_build;
mod ident_located;
mod ident_version;
pub mod parsing;
mod range_ident;
mod request;
mod satisfy;

pub use error::{Error, Result};
pub use ident::Ident;
pub use ident_any::{parse_ident, AnyIdent};
pub use ident_build::{parse_build_ident, BuildIdent};
pub use ident_located::{LocatedBuildIdent, LocatedVersionIdent};
pub use ident_version::{parse_version_ident, VersionIdent};
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
pub use satisfy::Satisfy;

pub mod prelude {
    pub use super::Satisfy;
}
