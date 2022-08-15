// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod compat;
mod error;
pub mod parsing;
mod version;

pub use compat::{
    parse_compat, Compat, CompatRule, CompatRuleSet, Compatibility, API_STR, BINARY_STR,
};
pub use error::{Error, Result};
pub use version::{
    get_version_position_label, parse_tag_set, parse_version, InvalidVersionError, TagSet, Version,
    VersionParts, VERSION_SEP,
};
