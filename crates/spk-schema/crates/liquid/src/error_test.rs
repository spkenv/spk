use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_error_position_extraction() {
    // ensure that the source position of an error can be
    // properly extracted and used for the returned serde_format_error

    format_serde_error::never_color();
    static TPL: &str = r#"{% default = data | replace ''%}"#;
    let err =
        crate::render_template(TPL, &json!({})).expect_err("expected template render to fail");
    let expected = r#"
 1 | {% default = data | replace ''%}
   |            ^ unexpected "="; expected Identifier
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
