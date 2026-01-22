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
            options:
              - {var: python.abi/cp37}
            script: echo "hello"
            "#,
        )
        .is_err()
    );
}

/// For backwards compatibility a build script is tolerated if it matches the
/// default build script.
///
/// The original way the code tested if a build script is missing was to test if
/// the value matched the default script but that doesn't cover when the build
/// script is present but still matches the default.
#[rstest]
fn default_script_is_allowed() {
    assert!(
        serde_yaml::from_str::<EmbeddedBuildSpec>(
            r#"
            options:
              - {var: python.abi/cp37}
            script:
              - sh ./build.sh
            "#,
        )
        .is_ok()
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
