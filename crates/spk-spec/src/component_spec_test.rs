// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spk_ident_component::Component;

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

#[rstest]
#[case("{name: valid, files: ['*.yaml']}")]
fn test_component_files_yaml_roundtrip(#[case] yaml: &str) {
    let spec = serde_yaml::from_str::<ComponentSpec>(yaml).unwrap();
    let inter = serde_yaml::to_string(&spec).unwrap();
    let spec2 = serde_yaml::from_str::<ComponentSpec>(&inter).unwrap();
    assert_eq!(spec, spec2, "expected no changes going through yaml");
}

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
