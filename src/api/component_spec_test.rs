// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::{ComponentSpec, FileMatcher};

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
#[case(&[], "/file.txt", false)]
#[case(&["/file.txt"], "/file.txt", true)]
#[case(&["*.txt"], "/data/file.txt", true)]
#[case(&["file.txt/"], "/data/file.txt", false)]
fn test_file_matcher_matching(
    #[case] patterns: &[&str],
    #[case] path: &str,
    #[case] should_match: bool,
) {
    // we're not really testing gitignore here, just that the
    // semantics of our function works as expected
    let matcher = FileMatcher::new(patterns.iter().map(|s| s.to_string())).unwrap();
    assert_eq!(matcher.matches(path, path.ends_with("/")), should_match);
}
