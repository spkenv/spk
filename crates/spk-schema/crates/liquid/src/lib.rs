// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Defines the default configuration for processing spec file templates in spk

mod error;
mod filter_compare_version;
mod filter_parse_version;
mod filter_replace_regex;
mod tag_default;

pub use format_serde_error::SerdeError as Error;

/// Build the default template parser for spk
///
/// This parser has all configuration and extensions
/// needed for rendering spk spec templates.
pub fn default_parser() -> liquid::Parser {
    let res = liquid::ParserBuilder::new()
        .stdlib()
        .tag(tag_default::DefaultTag)
        .filter(filter_parse_version::ParseVersion)
        .filter(filter_compare_version::CompareVersion)
        .filter(filter_replace_regex::ReplaceRegex)
        .build();
    debug_assert!(res.is_ok(), "default template parser is valid");
    res.unwrap()
}

/// Render a template with the default configuration
pub fn render_template<T, D>(tpl: T, data: &D) -> Result<String, Error>
where
    T: AsRef<str>,
    D: serde::Serialize,
{
    let tpl = tpl.as_ref();
    let map_err =
        |err| format_serde_error::SerdeError::new(tpl.to_string(), error::to_error_types(err));
    let parser = default_parser();
    let template = parser.parse(tpl).map_err(map_err)?;
    let globals = liquid::to_object(data).map_err(map_err)?;
    template.render(&globals).map_err(map_err)
}
