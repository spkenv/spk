// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::InstallSpec;

#[rstest]
fn test_install_embedded_build_options() {
    let _spec: InstallSpec = serde_yaml::from_str(
        r#"
        embedded:
          - pkg: "embedded/1.0.0"
            build: {"options": [{"var": "python.abi", "static": "cp37"}]}
        "#,
    )
    .unwrap();

    assert!(serde_yaml::from_str::<InstallSpec>(
        r#"
        embedded:
          - pkg: "embedded/1.0.0"
            build: {"script": "echo hello"}
        "#
    )
    .is_err());
}
