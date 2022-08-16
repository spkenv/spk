// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::Component;

#[rstest]
fn test_component_name_serialize() {
    assert_eq!(Component::All, serde_yaml::from_str("all").unwrap());
    assert_eq!(Component::Run, serde_yaml::from_str("run").unwrap());
    assert_eq!(Component::Build, serde_yaml::from_str("build").unwrap());
    assert_eq!(Component::Source, serde_yaml::from_str("src").unwrap());
    assert_eq!(
        Component::Named("other".into()),
        serde_yaml::from_str("other").unwrap()
    );
}
