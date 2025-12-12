// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema_foundation::{option_map, pkg_name};

use super::BuildSpec;

#[rstest]
fn test_options_with_components_produce_different_build_ids() {
    let yaml1 = r#"
options:
  - pkg: pkg:comp1
"#;
    let yaml2 = r#"
options:
  - pkg: pkg:comp2
"#;
    let res1 = serde_yaml::from_str::<BuildSpec>(yaml1).unwrap();
    let res2 = serde_yaml::from_str::<BuildSpec>(yaml2).unwrap();
    let build_id1 = res1
        .build_digest(pkg_name!("dummy"), &option_map! {})
        .unwrap();
    let build_id2 = res2
        .build_digest(pkg_name!("dummy"), &option_map! {})
        .unwrap();
    assert_ne!(build_id1, build_id2);
}
