// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::Opt;
use crate::foundation::FromYaml;

#[rstest]
#[case("{pkg: my-pkg}", "1", false)]
#[case("{pkg: my-pkg}", "none", true)]
#[case("{pkg: my-pkg}", "", false)]
#[case("{pkg: my-pkg}", "1.0.0/QYB6QLCN", false)]
#[case("{pkg: my-pkg}", "2", false)]
#[case("{pkg: my-pkg}", "2.7", false)]
#[case("{pkg: my-pkg}", "~2.7", false)]
#[case("{pkg: my-pkg}", "2,<3", false)]
#[case("{pkg: my-pkg}", "2,3", false)]
#[case("{pkg: my-pkg}", "2.7,<3", false)]
#[case("{pkg: my-pkg}", "<3", false)]
#[case("{pkg: my-pkg}", ">3", false)]
fn test_pkg_opt_validation(#[case] spec: &str, #[case] value: &str, #[case] expect_err: bool) {
    let mut opt = Opt::from_yaml(spec).unwrap().into_pkg().unwrap();
    let res = opt.set_value(value.to_string());
    assert_eq!(res.is_err(), expect_err, "{res:?}");
}

#[rstest]
#[case("{var: my-var, choices: [hello, world]}", "hello", false)]
#[case("{var: my-var, choices: [hello, world]}", "bad", true)]
#[case("{var: my-var, choices: [hello, world]}", "", false)]
fn test_var_opt_validation(#[case] spec: &str, #[case] value: &str, #[case] expect_err: bool) {
    let mut opt = Opt::from_yaml(spec).unwrap().into_var().unwrap();
    let res = opt.set_value(value.to_string());
    assert_eq!(res.is_err(), expect_err);
}

#[rstest]
#[case("{var: my-var, default: value}", Some("value"))] // deprecated, but still supported
#[case("{var: my-var/value}", Some("value"))]
#[case("{var: my-var}", None)]
#[case("{var: my-var/}", None)] // empty is mapped to none
#[case("{static: static, var: my-var}", Some("static"))] // static instead of default
#[case("{static: static, var: my-var/default}", Some("static"))] // static supersedes default
fn test_var_opt_parse_value(#[case] spec: &str, #[case] expected: Option<&str>) {
    let opt = Opt::from_yaml(spec).unwrap().into_var().unwrap();
    let actual = opt.get_value(None);
    assert_eq!(actual.as_deref(), expected);
}

/// Confirm that the error provided when both 'var' or 'pkg' field
/// exist is meaningful and positioned reasonably
#[rstest]
fn test_yaml_error_ambiguous() {
    format_serde_error::never_color();
    // use a vector of options just to confirm the error positioning
    static YAML: &str = r#"- var: os/linux
- var: hello
  pkg: hello
"#;
    let err = Vec::<Opt>::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
   | - var: os/linux
 2 | - var: hello
   |   ^ .[1]: could not determine option type, it may only contain one of the `pkg` or `var` fields at line 2 column 3
   |   pkg: hello
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}

/// Confirm that the error provided when no 'var' or 'pkg' field
/// exists is meaningful and positioned reasonably
#[rstest]
fn test_yaml_error_undetermined() {
    format_serde_error::never_color();
    // use a vector of options just to confirm the error positioning
    static YAML: &str = r#"- var: os/linux
- static: value
  prereleasePolicy: IncludeAll
"#;
    let err = Vec::<Opt>::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
   | - var: os/linux
 2 | - static: value
   |   ^ .[1]: could not determine option type, it must include either a `pkg` or `var` field at line 2 column 3
   |   prereleasePolicy: IncludeAll
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
