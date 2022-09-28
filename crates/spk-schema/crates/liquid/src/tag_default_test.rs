// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_default_value() {
    // because we require a value for all variables in the template
    // a helper must be provided that allows for defining default values
    // as a convenience

    let options = json!({"version": "2.3.4"});
    static TPL: &str = r#"
{% default name = "my-package" %}
{% default version = "ignore-me" %}
pkg: {{ name }}/{{ version }}
sources:
  - git: https://downloads.testing/{{ name }}/v{{ version }}
"#;
    static EXPECTED: &str = r#"


pkg: my-package/2.3.4
sources:
  - git: https://downloads.testing/my-package/v2.3.4
"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_template_rendering_defaults_many() {
    // ensure that using multiple defaults does not
    // cause them to interfere

    let options = json!({});
    static TPL: &str = r#"
{% default name = "my-package" %}
{% default version = "2.3.4" %}
pkg: {{ name }}/{{ version }}
sources:
  - git: https://downloads.testing/{{ name }}/v{{ version }}
"#;
    static EXPECTED: &str = r#"


pkg: my-package/2.3.4
sources:
  - git: https://downloads.testing/my-package/v2.3.4
"#;
    let rendered =
        crate::render_template(TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
