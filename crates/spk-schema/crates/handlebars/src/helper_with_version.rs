use std::{ops::Deref, str::FromStr};

use handlebars::{BlockContext, BlockParams, Renderable};
use serde_json::json;
use spk_schema_foundation::version::Version;

use crate::params::string_param;

#[cfg(test)]
#[path = "./helper_with_version_test.rs"]
mod helper_with_version_test;

/// Breaks an spk version into components for access to it's pieces
#[derive(Clone, Copy)]
pub struct WithVersionHelper;

impl handlebars::HelperDef for WithVersionHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &handlebars::Helper<'reg, 'rc>,
        r: &'reg handlebars::Handlebars<'reg>,
        ctx: &'rc handlebars::Context,
        rc: &mut handlebars::RenderContext<'reg, 'rc>,
        out: &mut dyn handlebars::Output,
    ) -> handlebars::HelperResult {
        let params = h.params();
        if params.len() != 1 {
            return Err(handlebars::RenderError::new(format!(
                "{} takes exactly one parameter: VERSION",
                h.name()
            )));
        }
        let version_param = string_param!("VERSION", &params[0]);

        let version = Version::from_str(version_param).map_err(|err| {
            handlebars::RenderError::new(format!(
                "expected a valid spk version number, got '{version_param}': {err}"
            ))
        })?;

        let data = json!({
            "major": version.major(),
            "minor": version.minor(),
            "patch": version.patch(),
            "base": version.base(),
            "parts": version.parts.to_vec(),
            "pre": version.pre.deref(),
            "post": version.post.deref(),
        });

        let mut block = BlockContext::new();
        if let Some(block_param) = h.block_param() {
            let mut block_params = BlockParams::new();
            block_params.add_value(block_param, data)?;
            block.set_block_params(block_params);
        } else {
            block.set_base_value(data);
        }
        rc.push_block(block);
        if let Some(t) = h.template() {
            t.render(r, ctx, rc, out)?;
        };
        rc.pop_block();
        Ok(())
    }
}
