use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_version_range() {
    // the compare_version helper should be useful in if blocks
    // in order to render section based on version ranges

    let options = json!({});
    static TPL: &str = r#"
{{ default version "1.2.3" }}
pkg: package/{{ version }}
sources:
{{#if-version version ">=1.0"}}
  - git: https://downloads.testing/package/v{{ version }}
{{/if-version}}
{{#if-version version "<1.0"}}
  - git: https://olddownloads.testing/package/v{{ version }}
{{/if-version}}
"#;
    static EXPECTED: &str = r#"
set default: version="1.2.3" [default: "1.2.3"]
pkg: package/1.2.3
sources:
  - git: https://downloads.testing/package/v1.2.3
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_template_rendering_version_range_reverse() {
    // the compare_version helper should be useful in if blocks
    // in order to render section based on version ranges

    let options = json!({});
    static TPL: &str = r#"
{{ default version "1.2.3" }}
pkg: package/{{ version }}
sources:
{{#unless-version version ">=1.0"}}
  - git: https://downloads.testing/package/v{{ version }}
{{/unless-version}}
{{#unless-version version "<1.0"}}
  - git: https://olddownloads.testing/package/v{{ version }}
{{/unless-version}}
"#;
    static EXPECTED: &str = r#"
set default: version="1.2.3" [default: "1.2.3"]
pkg: package/1.2.3
sources:
  - git: https://olddownloads.testing/package/v1.2.3
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
