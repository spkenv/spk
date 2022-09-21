use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_replace_value() {
    // because we require a value for all variables in the template
    // a helper must be provided that allows for defining default values
    // as a convenience

    let options = json!({"package": "my_package"});
    static TPL: &str = r#"
{{ default version "1.2.3" }}
{{ default underscore_version (replace version "." "_") }}
pkg: {{ replace package "_" "-" }}/{{ version }}
sources:
  - git: https://downloads.testing/{{ package }}/v{{ underscore_version }}
"#;
    static EXPECTED: &str = r#"


pkg: my-package/1.2.3
sources:
  - git: https://downloads.testing/my_package/v1_2_3
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
