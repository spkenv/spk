// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ops::Deref;
use std::str::FromStr;

use liquid::ValueView;
use liquid_core::runtime::StackFrame;
use liquid_core::{
    Display_filter,
    Expression,
    Filter,
    FilterParameters,
    FilterReflection,
    FromFilterParameters,
    ParseFilter,
    Result,
    Runtime,
    Value,
};
use spk_schema_foundation::version::Version;

#[cfg(test)]
#[path = "./filter_parse_version_test.rs"]
mod filter_parse_version_test;

#[derive(Debug, FilterParameters)]
struct ParseVersionArgs {
    #[parameter(description = "An optional sub-component to access", arg_type = "str")]
    path: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "parse_version",
    description = "Parses an spk version, outputting one or all components",
    parameters(ParseVersionArgs),
    parsed(ParseVersionFilter)
)]
pub struct ParseVersion;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "parse_version"]
struct ParseVersionFilter {
    #[parameters]
    args: ParseVersionArgs,
}

impl Filter for ParseVersionFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let input = input.as_scalar().ok_or_else(|| {
            liquid::Error::with_msg("Expected a scalar value for filter 'parse_version'")
        })?;
        let version = Version::from_str(input.into_string().as_str())
            .map_err(|err| liquid::Error::with_msg(err.to_string()))?;

        let data = liquid::object!({
            "major": version.major(),
            "minor": version.minor(),
            "patch": version.patch(),
            "base": version.base(),
            "parts": version.parts.parts,
            "plus_epsilon": version.parts.plus_epsilon,
            "pre": version.pre.deref(),
            "post": version.post.deref(),
        });

        if let Some(path) = &self.args.path {
            if matches!(path, Expression::Literal(..)) {
                return Err(liquid::Error::with_msg(
                    "parse_version expected a path to evaluate, but found a literal value",
                ));
            }
            let rt = StackFrame::new(runtime, data);
            let value = path.evaluate(&rt)?;
            Ok(value.into_owned())
        } else {
            Ok(data.into())
        }
    }
}
