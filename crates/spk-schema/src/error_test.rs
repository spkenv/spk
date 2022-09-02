// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

#[rstest]
fn test_yaml_error_empty() {
    static YAML: &str = r#""#;
    let err = crate::Spec::from_yaml(YAML).expect_err("expected yaml parsing to fail");

    let message = err.to_string();
    assert_eq!(
        message, "Yaml was completely empty",
        "should explain that the yaml was empty"
    );
}

#[rstest]
fn test_yaml_short() {
    static YAML: &str = r#"short and wrong"#;
    let err = crate::Spec::from_yaml(YAML).expect_err("expected yaml parsing to fail");

    let expected = r#"invalid type: string "short and wrong", expected struct YamlMapping at line 1 column 1
000 | short and wrong
    | ^
"#;

    let message = err.to_string();
    assert_eq!(message, expected);
}


#[rstest]
fn test_yaml_longer_context() {
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
    let err = crate::Spec::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"install.requirements: invalid type: map, expected a sequence at line 10 column 17
007 |     - "hello, world"
008 | install:
009 |   requirements: {}
    |                 ^
010 | test:
011 |   - stage: 1
"#;

    let message = err.to_string();
    assert_eq!(message, expected);
}
