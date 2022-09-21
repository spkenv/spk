// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spk_schema_foundation::{name::PkgName, option_map};

use crate::Template;

use super::SpecTemplate;

#[rstest]
fn test_template_error_position() {
    format_serde_error::never_color();
    static SPEC: &str = r#"pkg: mypackage/{{ version }}
sources:
  - git: https://downloads.testing/mypackage/v{{ verison }}
"#;
    let tpl = SpecTemplate {
        name: PkgName::new("my-package").unwrap().to_owned(),
        file_path: "my-package.spk.yaml".into(),
        template: SPEC.to_string(),
    };
    let options = option_map! {"version" => "1.0.0"};
    let err = tpl
        .render(&options)
        .expect_err("expect template rendering to fail");
    let expected = r#"
   | pkg: mypackage/{{ version }}
   | sources:
 3 |   - git: https://downloads.testing/mypackage/v{{ verison }}
   |                                                ^ Error rendering "my-package.spk.yaml" line 3, col 47: Variable "verison" not found in strict mode.
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
