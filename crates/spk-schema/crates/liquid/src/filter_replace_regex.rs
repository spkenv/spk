// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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

#[cfg(test)]
#[path = "./filter_replace_regex_test.rs"]
mod filter_replace_regex_test;

#[derive(Debug, FilterParameters)]
struct ReplaceRegexArgs {
    #[parameter(description = "The regular expression to search.", arg_type = "str")]
    search: Expression,
    #[parameter(
        description = "The text to replace search results with. If not given, the filter will just delete search results. Capture groups can be substituted using `$<name_or_number>`",
        arg_type = "str"
    )]
    replace: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "replace_re",
    description = "Like `replace`, but searches using a regular expression.",
    parameters(ReplaceRegexArgs),
    parsed(ReplaceRegexFilter)
)]
pub struct ReplaceRegex;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "replace_re"]
struct ReplaceRegexFilter {
    #[parameters]
    args: ReplaceRegexArgs,
}

impl Filter for ReplaceRegexFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;

        let input = input.to_kstr();

        let search = regex::Regex::new(&args.search)
            .map_err(|err| liquid::Error::with_msg(err.to_string()))?;
        let replace = args.replace.unwrap_or_else(|| "".into());

        Ok(Value::scalar(
            search.replace_all(&input, replace.as_str()).to_string(),
        ))
    }
}
