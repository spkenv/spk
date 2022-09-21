use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_simple() {
    static TPL: &str = r#"pkg: mypackage/{{ version }}
sources:
  - git: https://downloads.testing/mypackage/v{{ version }}
"#;
    let options = json!({"version": "2.3.4"});
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert!(rendered.contains("mypackage/2.3.4"));
}

#[rstest]
fn test_template_rendering_default_value() {
    // because we require a value for all variables in the template
    // a helper must be provided that allows for defining default values
    // as a convenience

    static TPL: &str = r#"
{{ default name "my-package" }}
pkg: {{ name }}/{{ version }}
sources:
  - git: https://downloads.testing/{{ name }}/v{{ version }}
"#;
    let options = json!({"version": "2.3.4"});
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert!(
        rendered.contains("my-package/2.3.4"),
        "the default value should be filled in"
    );
}
