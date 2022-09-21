use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_replace_value() {
    let options = json!({"package": "my_package"});
    static TPL: &str = r#"
{{ default version "1.2.3" }}
{{ default underscore_version (replace version "." "_") }}
pkg: {{ replace package "_" "-" }}/{{ version }}
sources:
  - git: https://downloads.testing/{{ package }}/v{{ underscore_version }}
"#;
    static EXPECTED: &str = r#"
set default: version="1.2.3" [default: "1.2.3"]
set default: underscore_version="1_2_3" [default: "1_2_3"]
pkg: my-package/1.2.3
sources:
  - git: https://downloads.testing/my_package/v1_2_3
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
