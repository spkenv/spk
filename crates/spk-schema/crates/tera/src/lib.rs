// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Defines the default configuration for processing spec file templates in spk

mod error;
mod filter_compare_version;
mod filter_default_options;
mod filter_parse_version;
mod filter_replace_regex;

pub use error::Error;

/// Build the default template rendered for spk
///
/// This parser has all configuration and extensions
/// needed for rendering spk spec templates.
pub fn default_renderer() -> tera::Tera {
    let mut renderer = tera::Tera::default();
    renderer.register_filter(
        filter_parse_version::ParseVersion::FILTER_NAME,
        filter_parse_version::ParseVersion,
    );
    renderer.register_filter(
        filter_compare_version::CompareVersion::FILTER_NAME,
        filter_compare_version::CompareVersion,
    );
    renderer.register_filter(
        filter_replace_regex::ReplaceRegex::FILTER_NAME,
        filter_replace_regex::ReplaceRegex,
    );
    renderer.register_filter(
        filter_default_options::DefaultOpts::FILTER_NAME,
        filter_default_options::DefaultOpts,
    );
    renderer
}

/// Render a template with the default configuration
pub fn render_template<N, T, D>(filename: N, tpl: T, data: &D) -> Result<String, Error>
where
    N: AsRef<str>,
    T: AsRef<str>,
    D: serde::Serialize,
{
    let tpl = tpl.as_ref();
    let mut tera = default_renderer();
    let map_err = |err| Error::build(tpl.to_string(), err);
    tera.add_raw_template(filename.as_ref(), tpl)
        .map_err(map_err)?;
    let context = tera::Context::from_serialize(data).map_err(map_err)?;
    let rendered = tera.render(filename.as_ref(), &context).map_err(map_err)?;
    Ok(rendered)
}
