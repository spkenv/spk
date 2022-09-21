use crate::params::string_param;

#[cfg(test)]
#[path = "./helper_replace_test.rs"]
mod helper_replace_test;

/// Allows for simple substring replacement in template variables
#[derive(Clone, Copy)]
pub struct ReplaceHelper;

impl handlebars::HelperDef for ReplaceHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &handlebars::Helper,
        _: &handlebars::Handlebars,
        _: &handlebars::Context,
        _: &mut handlebars::RenderContext,
        out: &mut dyn handlebars::Output,
    ) -> handlebars::HelperResult {
        let params = h.params();
        if params.len() != 3 {
            return Err(handlebars::RenderError::new(
                "the replace helper takes exactly three parameters: VALUE, MATCH and REPLACE",
            ));
        }
        let value_param = string_param!("VALUE", &params[0]);
        let match_param = string_param!("MATCH", &params[1]);
        let replace_param = string_param!("REPLACE", &params[2]);

        out.write(&value_param.replace(match_param, replace_param))?;
        Ok(())
    }
}
