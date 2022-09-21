use std::str::FromStr;

use handlebars::Renderable;
use spk_schema_foundation::{
    version::Version,
    version_range::{Ranged, VersionFilter},
};

use crate::params::string_param;

#[cfg(test)]
#[path = "./helper_version_compare_test.rs"]
mod helper_version_compare_test;

/// Allows for the comparison of spk version numbers
#[derive(Clone, Copy)]
pub struct IfVersionHelper;

impl handlebars::HelperDef for IfVersionHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &handlebars::Helper<'reg, 'rc>,
        r: &'reg handlebars::Handlebars<'reg>,
        ctx: &'rc handlebars::Context,
        rc: &mut handlebars::RenderContext<'reg, 'rc>,
        out: &mut dyn handlebars::Output,
    ) -> Result<(), handlebars::RenderError> {
        let params = h.params();
        if !(2..3).contains(&params.len()) {
            return Err(handlebars::RenderError::new(format!(
                "{} takes only two or three parameters: VERSION, RANGE_OR_OPERATOR, [RHS]",
                h.name()
            )));
        }
        let lhs_param = string_param!("VERSION", &params[0]);
        let op_param = string_param!("RANGE_OR_OPERATOR", &params[1]);
        let rhs_param = match params.get(2) {
            Some(p) => string_param!("RHS", p),
            None => "",
        };

        let lhs = Version::from_str(lhs_param).map_err(|err| {
            handlebars::RenderError::new(format!(
                "expected a valid spk version number, got '{lhs_param}': {err}"
            ))
        })?;
        let range = format!("{op_param}{rhs_param}");
        let rhs = VersionFilter::from_str(&range).map_err(|err| {
            handlebars::RenderError::new(format!(
                "expected a valid spk version range, got '{range}': {err}"
            ))
        })?;
        let tpl = if rhs.is_applicable(&lhs).is_ok() {
            h.template()
        } else {
            h.inverse()
        };
        match tpl {
            Some(t) => t.render(r, ctx, rc, out),
            None => Ok(()),
        }
    }
}

/// Allows for the inverse comparison of spk version numbers
#[derive(Clone, Copy)]
pub struct UnlessVersionHelper;

impl handlebars::HelperDef for UnlessVersionHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &handlebars::Helper<'reg, 'rc>,
        r: &'reg handlebars::Handlebars<'reg>,
        ctx: &'rc handlebars::Context,
        rc: &mut handlebars::RenderContext<'reg, 'rc>,
        out: &mut dyn handlebars::Output,
    ) -> Result<(), handlebars::RenderError> {
        let params = h.params();
        if !(2..3).contains(&params.len()) {
            return Err(handlebars::RenderError::new(format!(
                "{} takes only two or three parameters: VERSION, RANGE_OR_OPERATOR, [RHS]",
                h.name()
            )));
        }
        let lhs_param = string_param!("VERSION", &params[0]);
        let op_param = string_param!("RANGE_OR_OPERATOR", &params[1]);
        let rhs_param = match params.get(2) {
            Some(p) => string_param!("RHS", p),
            None => "",
        };

        let lhs = Version::from_str(lhs_param).map_err(|err| {
            handlebars::RenderError::new(format!(
                "expected a valid spk version number, got '{lhs_param}': {err}"
            ))
        })?;
        let range = format!("{op_param}{rhs_param}");
        let rhs = VersionFilter::from_str(&range).map_err(|err| {
            handlebars::RenderError::new(format!(
                "expected a valid spk version range, got '{range}': {err}"
            ))
        })?;
        let tpl = if rhs.is_applicable(&lhs).is_ok() {
            h.inverse()
        } else {
            h.template()
        };
        match tpl {
            Some(t) => t.render(r, ctx, rc, out),
            None => Ok(()),
        }
    }
}
