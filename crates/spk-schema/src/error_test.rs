// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::InvalidYamlError;

#[rstest]
fn test_yaml_error_empty() {
    static YAML: &str = r#""#;
    let res = serde_yaml::from_str::<crate::Spec>(YAML);
    let err = match res {
        Err(err) => InvalidYamlError {
            yaml: YAML.to_string(),
            err,
        },
        Ok(_) => panic!("expected yaml parsing to fail"),
    };

    let message = err.to_string();
    assert_eq!(
        message, "Yaml was completely empty",
        "should explain that the yaml was empty"
    );
}

#[rstest]
fn test_yaml_short() {
    static YAML: &str = r#"short and wrong"#;
    let res = serde_yaml::from_str::<crate::Spec>(YAML);
    let err = match res {
        Err(err) => InvalidYamlError {
            yaml: YAML.to_string(),
            err,
        },
        Ok(_) => panic!("expected yaml parsing to fail"),
    };

    let expected = r#"invalid type: string "short and wrong", expected a YAML mapping at line 1 column 1
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
    let res = serde_yaml::from_str::<crate::Spec>(YAML);
    let err = match res {
        Err(err) => InvalidYamlError {
            yaml: YAML.to_string(),
            err,
        },
        Ok(_) => panic!("expected yaml parsing to fail"),
    };

    let expected = r#"invalid type: map, expected a sequence"#;

    let message = err.to_string();
    assert_eq!(message, expected);
}
