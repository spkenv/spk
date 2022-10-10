// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use crate::FromYaml;

#[rstest]
fn test_yaml_error_empty() {
    format_serde_error::never_color();
    static YAML: &str = r#""#;
    let err = crate::Spec::from_yaml(YAML).expect_err("expected yaml parsing to fail");

    let message = err.to_string();
    assert_eq!(
        message, "EOF while parsing a value\n",
        "should explain that the yaml ended prematurely"
    );
}

#[rstest]
fn test_yaml_short() {
    format_serde_error::never_color();
    static YAML: &str = r#"short and wrong"#;
    let err = crate::Spec::from_yaml(YAML).expect_err("expected yaml parsing to fail");

    let expected = r#"
 1 | short and wrong
   | ^ invalid type: string "short and wrong", expected struct YamlMapping at line 1 column 1
"#;

    let message = err.to_string();
    assert_eq!(message, expected);
}

#[rstest]
fn test_yaml_longer_context() {
    format_serde_error::never_color();
    static YAML: &str = r#"
api: v0/package
pkg: test
build:
  options:
    - pkg: something
  script:
    - "hello, world"
install:
  requirements: {}
test:
  - stage: 1
"#;
    let err = crate::SpecRecipe::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
    |   script:
    |     - "hello, world"
    | install:
 10 |   requirements: {}
    |                 ^ install.requirements: invalid type: map, expected a list of requirements at line 10 column 17
    | test:
    |   - stage: 1
"#;

    let message = err.to_string();
    assert_eq!(message, expected);
}
