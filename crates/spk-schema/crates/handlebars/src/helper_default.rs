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
        context: &handlebars::Context,
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

        let variable_name = match variable_name_param.relative_path() {
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

        let path_length = variable_name.len();
        let mut context = context.clone();
        let mut json_data = context.data_mut();
        let mut current_path = Vec::with_capacity(path_length);
        for (step_count, step_name) in variable_name.into_iter().enumerate() {
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
            if step_count + 1 < path_length {
                if json_object.get(&step_name).is_none() {
                    json_object.insert(
                        step_name.clone(),
                        serde_json::Value::Object(Default::default()),
                    );
                }
                json_data = json_object.get_mut(&step_name).unwrap();
                current_path.push(step_name);
                continue;
            }

            tracing::debug!(
                "using default value for '{}.{step_name}': {}",
                current_path.join("."),
                default_value_param.value().to_string()
            );
            json_object.insert(step_name, default_value_param.value().clone());
            rc.set_context(context);
            return Ok(());
        }

        tracing::debug!(
            "no default needed for '{}', which is set to {}",
            current_path.join("."),
            json_data.to_string()
        );
        Ok(())
    }
}
