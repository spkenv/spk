macro_rules! string_param {
    ($name:literal, $param:expr) => {
        match $param.value().as_str() {
            Some(v) => v,
            None => {
                return Err(handlebars::RenderError::new(format!(
                    "expected a string for parameter {}",
                    $name
                )));
            }
        }
    };
}

pub(crate) use string_param;
