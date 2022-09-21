//! Defines the default configuration for processing spec file templates in spk

mod helper_default;
mod helper_replace;
mod helper_version_compare;
mod params;

/// Build the default handlebars registry for spk
///
/// This registry has all configuration set and expected helpers
/// registered for rendering spk spec templates.
pub fn default_registry() -> handlebars::Handlebars<'static> {
    let mut reg = handlebars::Handlebars::new();
    // do not allow unresolved template variables when rendering,
    // all template items must be filled in.
    reg.set_strict_mode(true);
    reg.register_helper("default", Box::new(helper_default::DefaultHelper));
    reg.register_helper("replace", Box::new(helper_replace::ReplaceHelper));
    reg.register_helper(
        "if-version",
        Box::new(helper_version_compare::IfVersionHelper),
    );
    reg.register_helper(
        "unless-version",
        Box::new(helper_version_compare::UnlessVersionHelper),
    );
    reg
}

/// Render a template with the default configuration
pub fn render_template<N, T, D>(
    name: N,
    tpl: T,
    data: &D,
) -> Result<String, handlebars::RenderError>
where
    N: Into<String>,
    T: AsRef<str>,
    D: serde::Serialize,
{
    let mut reg = default_registry();
    let mut template = handlebars::Template::compile(tpl.as_ref())?;
    let name = name.into();
    template.name = Some(name.clone());
    reg.register_template(&name, template);
    reg.render(&name, data)
}
