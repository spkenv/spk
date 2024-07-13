// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use crate::v0::TestSpec;

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
