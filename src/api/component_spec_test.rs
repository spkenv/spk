// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::ComponentSpec;

#[rstest]
#[case("valid")]
#[should_panic]
#[case("invalid!")]
#[should_panic]
#[case("in_valid")]
fn test_component_name_validation(#[case] name: &str) {
    ComponentSpec::new(name).unwrap();
}

#[rstest]
#[case("name: valid")]
#[should_panic]
#[case("name: invalid!")]
#[should_panic]
#[case("name: in_valid")]
fn test_component_name_validation_deserialize(#[case] yaml: &str) {
    serde_yaml::from_str::<ComponentSpec>(yaml).unwrap();
}
