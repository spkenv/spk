// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_replace_regex_basic() {
    let options = json!({});
    static TPL: &str = r#"{{ "1992-02-25" | replace_re: "(\d+)-(\d+)-(\d+)", "$3/$2/$1" }}"#;
    static EXPECTED: &str = r#"25/02/1992"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_replace_regex_empty() {
    let options = json!({});
    static TPL: &str = r#"{{ "Hello, World!" | replace_re: "[A-Z]" }}"#;
    static EXPECTED: &str = r#"ello, orld!"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_replace_regex_compile_error() {
    let options = json!({"version": "1.2.3.4.5-beta.1+r.0"});
    static TPL: &str = r#"{{ "something" | replace_re: "(some]" }}"#;
    static EXPECTED_ERR: &str = r#"
liquid: regex parse error:
    (some]
    ^
error: unclosed group
from: Filter error
  with:
    filter=replace_re : "(some]"
    input="something"
"#;
    let err = crate::render_template(TPL, &options).expect_err("template should fail on bad regex");
    assert_eq!(err.to_string().trim(), EXPECTED_ERR.trim());
}
