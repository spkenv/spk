// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::OptionMap;
use crate::{opt_name, option_map, pkg_name, FromYaml};

#[rstest]
fn test_get_for_package() {
    let mut options = OptionMap::default();
    options.insert(opt_name!("namespace.option").to_owned(), "ns_opt".into());
    options.insert(opt_name!("option").to_owned(), "global_opt".into());
    options.insert(opt_name!("other.option").to_owned(), "other_opt".into());
    assert_eq!(
        options
            .get_for_package(pkg_name!("namespace"), opt_name!("option"))
            .unwrap(),
        "ns_opt"
    );
    assert_eq!(
        options
            .get_for_package(pkg_name!("nothing"), opt_name!("option"))
            .unwrap(),
        "global_opt"
    );
    assert_eq!(
        options
            .get_for_package(pkg_name!("namespace"), opt_name!("other.option"))
            .unwrap(),
        "other_opt"
    );
}

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
    assert_eq!(
        opts.options.get(opt_name!("one")).map(String::to_owned),
        Some("one".to_string())
    );
    assert_eq!(
        opts.options.get(opt_name!("two")).map(String::to_owned),
        Some("2".to_string())
    );
    assert_eq!(
        opts.options.get(opt_name!("three")).map(String::to_owned),
        Some("false".to_string())
    );
    assert_eq!(
        opts.options.get(opt_name!("four")).map(String::to_owned),
        Some("4.4".to_string())
    );
}

#[rstest]
fn test_yaml_error_context() {
    format_serde_error::never_color();
    static YAML: &str = r#"{option1: value, option2: oops: value}"#;
    let err = OptionMap::from_yaml(YAML).expect_err("expected yaml parsing to fail");
    let expected = r#"
 1 | {option1: value, option2: oops: value}
   |                               ^ while parsing a flow mapping, did not find expected ',' or '}' at line 1 column 31
"#;
    let message = err.to_string();
    assert_eq!(message, expected);
}
