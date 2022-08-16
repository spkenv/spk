// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::OptionMap;
use crate::option_map;
use crate::{opt_name, pkg_name};

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
