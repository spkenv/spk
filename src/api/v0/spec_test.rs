// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::Write;

use rstest::rstest;

use super::Spec;
use crate::{
    api::{self, OptionMap, Recipe, SpecTemplate, Template},
    fixtures::*,
};

#[rstest]
fn test_spec_is_valid_with_only_name() {
    let _spec: Spec = serde_yaml::from_str("{pkg: test-pkg}").unwrap();
}

#[rstest]
fn test_explicit_no_sources() {
    let spec: Spec = serde_yaml::from_str("{pkg: test-pkg, sources: []}").unwrap();
    assert!(spec.sources.is_empty());
}

#[rstest]
fn test_sources_relative_to_spec_file(tmpdir: tempfile::TempDir) {
    let spec_dir = tmpdir.path().canonicalize().unwrap().join("dir");
    std::fs::create_dir(&spec_dir).unwrap();
    let spec_file = spec_dir.join("package.spk.yaml");
    let mut file = std::fs::File::create(&spec_file).unwrap();
    file.write_all(b"{pkg: test-pkg}").unwrap();
    drop(file);

    let api::Spec::V0Package(spec) = SpecTemplate::from_file(&spec_file)
        .unwrap()
        .render(&OptionMap::default())
        .unwrap()
        .generate_source_build(&spec_dir)
        .unwrap();
    if let Some(super::SourceSpec::Local(local)) = spec.sources.get(0) {
        assert_eq!(local.path, spec_dir);
    } else {
        panic!("expected spec to have one local source spec");
    }
}
