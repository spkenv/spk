// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::error::Error;

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_replace_regex_basic() {
    let options = json!({});
    static TPL: &str =
        r#"{{ "1992-02-25" | replace_regex(from="(\d+)-(\d+)-(\d+)", to="$3/$2/$1") }}"#;
    static EXPECTED: &str = r#"25/02/1992"#;
    let rendered =
        crate::render_template("test", TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_replace_regex_empty() {
    let options = json!({});
    static TPL: &str = r#"{{ "Hello, World!" | replace_regex(from="[A-Z]") }}"#;
    static EXPECTED: &str = r#"ello, orld!"#;
    let rendered =
        crate::render_template("test", TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_replace_regex_compile_error() {
    let options = json!({"version": "1.2.3.4.5-beta.1+r.0"});
    static TPL: &str = r#"{{ "something" | replace_regex(from="(some]") }}"#;
    let err = crate::render_template("test", TPL, &options)
        .expect_err("template should fail on bad regex");
    let mut root = err.source().expect("error should have source");
    while let Some(source) = root.source() {
        root = source;
    }
    println!("error source: {root:?}");
    root.downcast_ref::<regex::Error>()
        .expect("should fail to parse regex");
}
