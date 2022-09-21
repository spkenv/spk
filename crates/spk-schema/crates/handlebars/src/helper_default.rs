#[cfg(test)]
#[path = "./helper_default_test.rs"]
mod helper_default_test;

/// Allows for specifying default values for template variables
///
/// Because we require that all variables to be filled in by default,
/// this allows "optional" template variables to exist by enabling
/// developers to set default values for variables that are not
/// otherwise passed in
#[derive(Clone, Copy)]
pub struct DefaultHelper;

impl handlebars::HelperDef for DefaultHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &handlebars::Helper,
        _: &handlebars::Handlebars,
        ctx: &handlebars::Context,
        rc: &mut handlebars::RenderContext,
        _: &mut dyn handlebars::Output,
    ) -> handlebars::HelperResult {
        let variable_name_param = match h.param(0) {
            Some(v) => v,
            None => {
                return Err(handlebars::RenderError::new(
                    "the default helper takes exactly two parameters: NAME and VALUE",
                ))
            }
        };
        let default_value_param = match h.param(1) {
            Some(v) => v,
            None => {
                return Err(handlebars::RenderError::new(
                    "the default helper takes exactly two parameters: NAME and VALUE",
                ))
            }
        };
        if h.param(2).is_some() {
            return Err(handlebars::RenderError::new(
                "the default helper takes exactly two parameters, found 3+",
            ));
        }

        let mut variable_name = match variable_name_param.relative_path() {
            // if a developer passes an un-quoted variable name, which is
            // the documented 'right way' to use this helper, then we want
            // to capture the actual name of the passed variable, not it's value
            Some(v) => v.split('.').map(str::to_string).collect::<Vec<_>>(),
            // but if no path is available, then it's assumed that a literal value
            // was passed (likely a quoted string)
            None => match variable_name_param.value().as_str() {
                Some(name) => name.split('.').map(str::to_string).collect(),
                None => {
                    return Err(handlebars::RenderError::new(
                        "expected a variable name or string literal as first parameter",
                    ));
                }
            },
        };

        let mut context = match rc.context() {
            Some(ctx) => (*ctx).clone(),
            None => ctx.clone(),
        };
        let mut json_data = context.data_mut();
        let mut current_path = Vec::with_capacity(variable_name.len());
        let final_step = variable_name.pop().ok_or_else(|| {
            handlebars::RenderError::new(format!(
                "first parameter cannot be an empty string or end with '.', got {}",
                variable_name.join(".")
            ))
        })?;
        for step_name in variable_name.into_iter() {
            if json_data.is_null() {
                *json_data = serde_json::Value::Object(Default::default());
            }
            let json_object = match json_data.as_object_mut() {
                Some(v) => v,
                None => {
                    return Err(handlebars::RenderError::new(format!(
                        "failed to set default: cannot descend into '{}' as it is not a mapping",
                        current_path.join(".")
                    )))
                }
            };
            if json_object.get(&step_name).is_none() {
                json_object.insert(
                    step_name.clone(),
                    serde_json::Value::Object(Default::default()),
                );
            }
            json_data = json_object.get_mut(&step_name).unwrap();
            current_path.push(step_name);
        }

        let json_object = match json_data.as_object_mut() {
            Some(v) => v,
            None => {
                return Err(handlebars::RenderError::new(format!(
                    "failed to set default: cannot descend into '{}' as it is not a mapping",
                    current_path.join(".")
                )))
            }
        };
        current_path.push(final_step.clone());
        if let Some(v) = json_object.get(&final_step) {
            tracing::debug!(
                "no default needed for '{}', which is set to {v}",
                current_path.join("."),
            );
            return Ok(());
        }
        tracing::debug!(
            "using default value for '{}': {}",
            current_path.join("."),
            default_value_param.value().to_string()
        );
        json_object.insert(final_step, default_value_param.value().clone());
        rc.set_context(context);
        Ok(())
    }
}
