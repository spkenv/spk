// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io::Write;

use rstest::rstest;

use super::Spec;
use crate::fixtures::*;

#[rstest]
fn test_empty_spec_is_valid() {
    let _spec: Spec = serde_yaml::from_str("{}").unwrap();
}

#[rstest]
fn test_explicit_no_sources() {
    let spec: Spec = serde_yaml::from_str("sources: []").unwrap();
    assert!(spec.sources.is_empty());
}

#[rstest]
fn test_sources_relative_to_spec_file(tmpdir: tempdir::TempDir) {
    let spec_dir = tmpdir.path().join("dir");
    std::fs::create_dir(&spec_dir).unwrap();
    let spec_file = spec_dir.join("package.spk.yaml");
    let mut file = std::fs::File::create(&spec_file).unwrap();
    file.write_all(b"{}").unwrap();
    drop(file);

    let spec = super::read_spec_file(&spec_file).unwrap();
    if let Some(super::SourceSpec::Local(local)) = spec.sources.get(0) {
        assert_eq!(local.path, spec_dir.join("."));
    } else {
        panic!("expected spec to have one local source spec");
    }
}
