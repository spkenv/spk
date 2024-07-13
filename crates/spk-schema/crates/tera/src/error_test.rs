// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_error_position_extraction() {
    // ensure that the source position of an error can be
    // properly extracted and used for the returned serde_format_error

    static TPL: &str = r#"{% default = data | replace ''%}"#;
    let err = crate::render_template("test", TPL, &json!({}))
        .expect_err("expected template render to fail");
    assert_eq!(
        err.labels().expect("labels").count(),
        1,
        "should capture original parsing error location"
    );
}
