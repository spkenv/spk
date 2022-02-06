// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::EmbeddedPackagesList;

#[rstest]
fn test_install_embedded_build_options() {
    let _spec: EmbeddedPackagesList = serde_yaml::from_str(
        r#"
          - pkg: "embedded/1.0.0"
            build: {"options": [{"var": "python.abi", "static": "cp37"}]}
        "#,
    )
    .unwrap();

    assert!(serde_yaml::from_str::<EmbeddedPackagesList>(
        r#"
          - pkg: "embedded/1.0.0"
            build: {"script": "echo hello"}
        "#
    )
    .is_err());
}
