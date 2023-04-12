// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_version_range() {
    // the compare_version helper should be useful in if blocks
    // in order to render section based on version ranges

    let options = json!({});
    static TPL: &str = r#"
{% default version = "1.2.3" %}
{% assign use_new_download = version | compare_version: ">=1.0" %}
pkg: package/{{ version }}
sources:
{%- if use_new_download %}
  - git: https://downloads.testing/package/v{{ version }}
{%- else %}
  - git: https://olddownloads.testing/package/v{{ version }}
{%- endif %}
"#;
    static EXPECTED: &str = r#"


pkg: package/1.2.3
sources:
  - git: https://downloads.testing/package/v1.2.3
"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
