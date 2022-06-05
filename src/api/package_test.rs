// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::{api::Spec, opt_name, option_map};

#[rstest]
fn test_resolve_options_package_option() {
    let spec: Spec = serde_yaml::from_str(
        r#"{
            api: v0/package,
            pkg: my-pkg/1.0.0,
            build: {
                options: [
                    {var: "python.abi/cp37m"},
                    {var: "my-opt/default"},
                    {var: "debug/off"},
                ]
            }
        }"#,
    )
    .unwrap();

    let options = option_map! {
        "python.abi" => "cp27mu",
        "my-opt" => "value",
        "my-pkg.my-opt" => "override",
        "debug" => "on",
    };
    let resolved = spec.resolve_options(&options);
    assert_eq!(
        resolved.get(opt_name!("my-opt")),
        Some(&"override".to_string()),
        "namespaced option should take precedence"
    );
    assert_eq!(
        resolved.get(opt_name!("debug")),
        Some(&"on".to_string()),
        "global opt should resolve if given"
    );
    assert_eq!(
        resolved.get(opt_name!("python.abi")),
        Some(&"cp27mu".to_string()),
        "opt for other package should exist"
    );
}
