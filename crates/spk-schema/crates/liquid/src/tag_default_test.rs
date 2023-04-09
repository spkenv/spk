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

#[rstest]
fn test_template_rendering_default_nested() {
    // ensure that setting a nested default value
    // works as expected

    let options = json!({
      "nested": {
        "existing": "existing"
      }
    });
    static TPL: &str = r#"
{% default nested.existing = "ignored" %}
{% default nested.other = "something" %}
pkg: {{ nested.other }}-{{ nested.existing }}
"#;
    static EXPECTED: &str = r#"


pkg: something-existing
"#;
    let rendered = match crate::render_template(TPL, &options) {
        Ok(r) => r,
        Err(err) => {
            println!("{err}");
            panic!("template should not fail to render");
        }
    };
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_template_rendering_default_nested_not_object() {
    // ensure that setting a nested default value
    // works as expected

    let options = json!({
      "integer": 64,
    });
    static TPL: &str = r#"
{% default integer.nested = "invalid" %}
"#;
    let err = crate::render_template(TPL, &options)
        .expect_err("Should fail when setting default under non-object");
    let expected = r#"liquid: Cannot set default
from: Stepping into non-object
  with:
    position=integer
    target=integer["nested"]

"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
