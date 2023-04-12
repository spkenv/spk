// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use liquid::model::Scalar;
use liquid::ValueView;
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
use spk_schema_foundation::version_range::{Ranged, VersionFilter};

#[cfg(test)]
#[path = "./filter_compare_version_test.rs"]
mod filter_compare_version_test;

#[derive(Debug, FilterParameters)]
struct CompareVersionArgs {
    #[parameter(description = "The comparison operation to perform", arg_type = "str")]
    operator: Expression,
    #[parameter(
        description = "The version to compare with, if not part of the operator string",
        arg_type = "str"
    )]
    rhs: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "compare_version",
    description = "Compares one version to another using spk ordering semantics",
    parameters(CompareVersionArgs),
    parsed(CompareVersionFilter)
)]
pub struct CompareVersion;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "compare_version"]
struct CompareVersionFilter {
    #[parameters]
    args: CompareVersionArgs,
}

impl Filter for CompareVersionFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;
        let input = input.as_scalar().ok_or_else(|| {
            liquid::Error::with_msg("Expected a scalar value for filter 'compare_version'")
        })?;
        let lhs = Version::from_str(input.into_string().as_str())
            .map_err(|err| liquid::Error::with_msg(err.to_string()))?;
        let range_str = match &args.rhs {
            None => args.operator.to_kstr().to_string(),
            Some(rhs) => format!("{}{}", args.operator, rhs),
        };
        let range = VersionFilter::from_str(range_str.as_str())
            .map_err(|err| liquid::Error::with_msg(err.to_string()))?;

        let result = range.is_applicable(&lhs).is_ok();
        Ok(Value::Scalar(Scalar::new(result).to_owned()))
    }
}
