// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_parse_version_access_basic() {
    let options = json!({});
    static TPL: &str = r#"{{ "1.2.3.4.5-beta.1+r.0" | parse_version: minor }}"#;
    static EXPECTED: &str = r#"2"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_parse_version_access_block_params() {
    let options = json!({"version": "1.2.3.4.5-beta.1+r.0"});
    static TPL: &str = r#"
{% assign v = version | parse_version %}
{{version}}
{{v.base}}
{{v.major}}
{{v.minor}}
{{v.patch}}
{{v.parts[3]}}
{{v.parts[4]}}
{{v.post.r}}
{{v.pre.beta}}
"#;
    static EXPECTED: &str = r#"
1.2.3.4.5-beta.1+r.0
1.2.3.4.5
1
2
3
4
5
0
1
"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered.trim(), EXPECTED.trim());
}
