// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::OptionMap;
use crate::option_map;

#[rstest]
fn test_package_options() {
    let mut options = OptionMap::default();
    options.insert("message".into(), "hello, world".into());
    options.insert("my-pkg.message".into(), "hello, package".into());
    assert_eq!(
        options.global_options(),
        option_map! {"message" => "hello, world"}
    );
    assert_eq!(
        options.package_options("my-pkg"),
        option_map! {"message" => "hello, package"}
    );
}
