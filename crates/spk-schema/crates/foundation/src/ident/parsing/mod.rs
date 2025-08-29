// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod ident;
mod request;

#[cfg(test)]
#[path = "./parsing_test.rs"]
mod parsing_test;

pub use ident::{build_ident, ident, opt_version_ident, version_ident};
pub use request::{
    range_ident,
    range_ident_comma_separated_list,
    range_ident_version_filter,
    version_filter_and_build,
};
