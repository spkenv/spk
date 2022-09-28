// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Defines the default configuration for processing spec file templates in spk

mod filter_compare_version;
mod filter_parse_version;
mod tag_default;

pub use liquid::Error;

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
        .build();
    debug_assert!(matches!(res, Ok(_)), "default template parser is valid");
    res.unwrap()
}

/// Render a template with the default configuration
pub fn render_template<T, D>(tpl: T, data: &D) -> Result<String, liquid::Error>
where
    T: AsRef<str>,
    D: serde::Serialize,
{
    let parser = default_parser();
    let template = parser.parse(tpl.as_ref())?;
    let globals = liquid::to_object(data)?;
    template.render(&globals)
}
