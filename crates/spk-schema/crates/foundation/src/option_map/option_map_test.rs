// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::OptionMap;
use crate::{opt_name, option_map, pkg_name, FromYaml};

#[rstest]
fn test_package_options() {
    let mut options = OptionMap::default();
    options.insert(opt_name!("message").to_owned(), "hello, world".into());
    options.insert(
        opt_name!("my-pkg.message").to_owned(),
        "hello, package".into(),
    );
    assert_eq!(
        options.global_options(),
        option_map! {"message" => "hello, world"}
    );
    assert_eq!(
        options.package_options(pkg_name!("my-pkg")),
        option_map! {"message" => "hello, package"}
    );
}

#[rstest]
fn test_option_map_deserialize_scalar() {
    let opts: OptionMap =
        serde_yaml::from_str("{one: one, two: 2, three: false, four: 4.4}").unwrap();
    assert_eq!(opts.options.get(opt_name!("one")), Some(&"one".into()));
    assert_eq!(opts.options.get(opt_name!("two")), Some(&"2".into()));
    assert_eq!(opts.options.get(opt_name!("three")), Some(&"false".into()));
    assert_eq!(opts.options.get(opt_name!("four")), Some(&"4.4".into()));
}

#[rstest]
fn test_yaml_error_context() {
    format_serde_error::never_color();
    static YAML: &str = r#"{option1: value, option2: oops: value}"#;
    let err = OptionMap::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
 1 | {option1: value, option2: oops: value}
   |                               ^ did not find expected ',' or '}' at line 1 column 31, while parsing a flow mapping
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
