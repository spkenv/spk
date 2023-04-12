// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use liquid::model::map::Entry;
use liquid::{Object, ValueView};
use liquid_core::error::ResultLiquidExt;
use liquid_core::parser::FilterChain;
use liquid_core::runtime::Variable;
use liquid_core::{
    Language,
    ParseTag,
    Renderable,
    Result,
    Runtime,
    TagReflection,
    TagTokenIter,
    Value,
};

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
        "assign a variable a value if it doesn't already have one"
    }
}

impl ParseTag for DefaultTag {
    fn parse(
        &self,
        mut arguments: TagTokenIter<'_>,
        options: &Language,
    ) -> Result<Box<dyn Renderable>> {
        let dst = arguments
            .expect_next("Variable expected.")?
            .expect_variable()
            .into_result()?;

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
    dst: Variable,
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

        let variable = self.dst.evaluate(runtime)?;
        let mut path = variable.iter().collect::<Vec<_>>().into_iter();
        let root = path.next().expect("at least one entry in path");

        let mut current_pos = liquid::model::Path::with_index(root.clone());
        let type_err = |pos: &liquid::model::Path| {
            liquid::Error::with_msg("Cannot set default")
                .trace("Stepping into non-object")
                .context("position", pos.to_string())
                .context("target", self.dst.to_string())
        };

        let Some(last) = path.next_back() else {
            if runtime.get(&[root.clone()]).is_err() {
                runtime.set_global(root.to_kstr().into(), value);
            }
            return Ok(());
        };
        let mut data = runtime
            .get(&[root.to_owned()])
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| Value::Object(Object::new()));
        let mut data_ref = &mut data;
        for step in path {
            data_ref = data_ref
                .as_object_mut()
                .ok_or_else(|| type_err(&current_pos))?
                .entry(step.to_kstr())
                .or_insert_with(|| Value::Object(Object::new()));
            current_pos.push(step.to_owned());
        }
        match data_ref
            .as_object_mut()
            .ok_or_else(|| type_err(&current_pos))?
            .entry(last.to_kstr())
        {
            Entry::Occupied(_) => {}
            Entry::Vacant(v) => {
                v.insert(value);
                runtime.set_global(root.to_kstr().into(), data);
            }
        }

        Ok(())
    }
}
