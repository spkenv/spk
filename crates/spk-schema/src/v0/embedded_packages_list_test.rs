// Copyright (c) Sony Pictures Imageworks, et al.
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

#[rstest]
fn test_embedded_nested_embedded() {
    // XXX Demonstrate that within an embedded package list, an embedded
    // package is able to define a component that embeds a nested package,
    // allowing for unbounded nesting of embedded packages.
    //
    // The embed stub code does not support creating stubs for nested embeds.
    // Decide if there is a use case for this and start supporting it, or
    // stop allowing this to be a valid package.
    let _spec: EmbeddedPackagesList = serde_yaml::from_str(
        r#"
          - pkg: "embedded/1.0.0"
            components:
              - name: run
                embedded:
                  - pkg: "nested-embedded/2.0.0"
        "#,
    )
    .unwrap();
}
