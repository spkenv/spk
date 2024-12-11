// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use crate::v0::TestSpec;
use crate::LintedItem;

#[rstest]
fn test_selectors_can_have_component_names() {
    let _test_spec: TestSpec = serde_yaml::from_str(
        r#"
stage: build
script:
  - true
selectors:
  - { "some-pkg:comp1": "1.0.0" }
    "#,
    )
    .expect("successfully parse selector with component specified");
}

#[rstest]
fn test_stage_and_script_lint() {
    let _test_spec: bool = serde_yaml::from_str::<LintedItem<TestSpec>>(
        r#"
stage: build
    "#,
    )
    .is_err();

    assert!(_test_spec);

    let _test_spec = serde_yaml::from_str::<LintedItem<TestSpec>>(
        r#"
script:
  - echo "Hello World!"
      "#,
    )
    .is_err();

    assert!(_test_spec);
}

#[rstest]
fn test_selectors_lint() {
    let _test_spec: LintedItem<TestSpec> = serde_yaml::from_str(
        r#"
stage: build
selector:
  - {gcc: 9.3}
script:
  - echo "Hello World!"
    "#,
    )
    .unwrap();

    assert_eq!(_test_spec.lints.len(), 1);
    for lint in _test_spec.lints.iter() {
        assert_eq!(lint.get_key(), "test.selector");
    }
}

#[rstest]
fn test_requirements_lint() {
    let _test_spec: LintedItem<TestSpec> = serde_yaml::from_str(
        r#"
stage: build
requirement:
  - pkg: foo
script:
  - echo "Hello World!"
    "#,
    )
    .unwrap();

    assert_eq!(_test_spec.lints.len(), 1);
    for lint in _test_spec.lints.iter() {
        assert_eq!(lint.get_key(), "test.requirement");
    }
}
