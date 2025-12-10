// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::EmbeddedBuildSpec;

#[rstest]
fn options_are_valid() {
    let _spec: EmbeddedBuildSpec = serde_yaml::from_str(
        r#"
      options:
        - {var: python.abi/cp37}
    "#,
    )
    .unwrap();
}

#[rstest]
fn script_is_invalid() {
    assert!(
        serde_yaml::from_str::<EmbeddedBuildSpec>(
            r#"
      script: echo "hello"
    "#,
        )
        .is_err()
    );
}

#[rstest]
fn unknown_field_is_allowed() {
    let _spec: EmbeddedBuildSpec = serde_yaml::from_str(
        r#"
      options:
        - {var: python.abi/cp37}
      unknown_field: some_value
    "#,
    )
    .unwrap();
}
