// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use liquid_core::error::ResultLiquidExt;
use liquid_core::parser::FilterChain;
use liquid_core::{Language, ParseTag, Renderable, Result, Runtime, TagReflection, TagTokenIter};

#[cfg(test)]
#[path = "./tag_default_test.rs"]
mod tag_default_test;

/// Allows for specifying default values for template variables
///
/// Because we require that all variables to be filled in by default,
/// this allows "optional" template variables to exist by enabling
/// developers to set default values for variables that are not
/// otherwise passed in
#[derive(Clone, Copy)]
pub struct DefaultTag;

impl TagReflection for DefaultTag {
    fn tag(&self) -> &'static str {
        "default"
    }

    fn description(&self) -> &'static str {
        "assign a variable value if it doesn't already have one"
    }
}

impl ParseTag for DefaultTag {
    fn parse(
        &self,
        mut arguments: TagTokenIter<'_>,
        options: &Language,
    ) -> Result<Box<dyn Renderable>> {
        let dst = arguments
            .expect_next("Identifier expected.")?
            .expect_identifier()
            .into_result()?
            .to_string()
            .into();

        arguments
            .expect_next("Assignment operator \"=\" expected.")?
            .expect_str("=")
            .into_result_custom_msg("Assignment operator \"=\" expected.")?;

        let src = arguments
            .expect_next("FilterChain expected.")?
            .expect_filter_chain(options)
            .into_result()?;

        // no more arguments should be supplied, trying to supply them is an error
        arguments.expect_nothing()?;

        Ok(Box::new(Default { dst, src }))
    }

    fn reflection(&self) -> &dyn TagReflection {
        self
    }
}

#[derive(Debug)]
struct Default {
    dst: liquid_core::model::KString,
    src: FilterChain,
}

impl Default {
    fn trace(&self) -> String {
        format!("{{% default {} = {}%}}", self.dst, self.src)
    }
}

impl Renderable for Default {
    fn render_to(&self, _writer: &mut dyn std::io::Write, runtime: &dyn Runtime) -> Result<()> {
        let value = self
            .src
            .evaluate(runtime)
            .trace_with(|| self.trace().into())?
            .into_owned();

        let name = self.dst.as_str().into();
        if runtime.try_get(&[name]).is_none() {
            runtime.set_global(self.dst.clone(), value);
        }
        Ok(())
    }
}
