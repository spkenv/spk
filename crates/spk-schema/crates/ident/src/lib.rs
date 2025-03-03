// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod format;
mod ident;
mod ident_any;
mod ident_build;
mod ident_located;
mod ident_optversion;
mod ident_version;
pub mod parsing;
mod range_ident;
mod request;
mod satisfy;

pub use error::{Error, Result};
pub use ident::{AsVersionIdent, Ident};
pub use ident_any::{AnyIdent, ToAnyIdentWithoutBuild, parse_ident};
pub use ident_build::{BuildIdent, parse_build_ident};
pub use ident_located::{LocatedBuildIdent, LocatedVersionIdent};
pub use ident_optversion::{OptVersionIdent, parse_optversion_ident};
pub use ident_version::{VersionIdent, parse_version_ident};
pub use range_ident::{RangeIdent, parse_ident_range, parse_ident_range_list};
pub use request::{
    InclusionPolicy,
    NameAndValue,
    PinPolicy,
    PinnableValue,
    PkgRequest,
    PreReleasePolicy,
    Request,
    RequestedBy,
    VarRequest,
    is_false,
};
pub use satisfy::Satisfy;

pub mod prelude {
    pub use super::Satisfy;
}
