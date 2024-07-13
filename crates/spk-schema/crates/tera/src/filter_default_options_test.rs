// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_template_rendering_default_value() {
    // because we require a value for all variables in the template
    // a helper must be provided that allows for defining default values
    // as a convenience

    let options = json!({"opt": {"version": "2.3.4"}});
    static TPL: &str = r#"
{% set opt = opt | default_opts(name="my-package", version="ignore-me") %}
pkg: {{ opt.name }}/{{ opt.version }}
sources:
  - git: https://downloads.testing/{{ opt.name }}/v{{ opt.version }}
"#;
    static EXPECTED: &str = r#"

pkg: my-package/2.3.4
sources:
  - git: https://downloads.testing/my-package/v2.3.4
"#;
    let rendered =
        crate::render_template("test", TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_template_rendering_defaults_many() {
    // ensure that using multiple defaults does not
    // cause them to interfere

    let options = json!({"opt": {}});
    static TPL: &str = r#"
{% set opt = opt | default_opts(name="my-package", version="2.3.4") %}
pkg: {{ opt.name }}/{{ opt.version }}
sources:
  - git: https://downloads.testing/{{ opt.name }}/v{{ opt.version }}
"#;
    static EXPECTED: &str = r#"

pkg: my-package/2.3.4
sources:
  - git: https://downloads.testing/my-package/v2.3.4
"#;
    let rendered =
        crate::render_template("test", TPL, &options).expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}
