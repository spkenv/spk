// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use rstest::rstest;

use super::EnvSpec;
use crate::fixtures::tmpdir;

#[rstest]
fn test_env_spec_validation() {
    let spec = EnvSpec::parse("one+two").expect("failed to parse env spec");
    assert_eq!(spec.items.len(), 2);
}

#[rstest]
fn test_env_spec_empty() {
    let empty = EnvSpec::parse("").expect("empty spec should be valid");
    let dash = EnvSpec::parse(super::ENV_SPEC_EMPTY).expect("dash spec should be valid");
    assert_eq!(
        empty, dash,
        "dash and empty string should be an empty spec (for cli parsing)"
    );
}

#[rstest]
fn test_env_spec_with_live_layer_dir(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("layer.spfs.yaml");
    let mut tmp_file = File::create(file_path).unwrap();
    writeln!(tmp_file, "# test live layer").unwrap();

    let env_spec = EnvSpec::parse(dir.display().to_string())
        .expect("absolute directory containing a layer.spfs.yaml should be valid");
    assert!(!env_spec.is_empty())
}

#[rstest]
fn test_env_spec_with_live_layer_file(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("layer.spfs.yaml");
    let mut tmp_file = File::create(file_path.clone()).unwrap();
    writeln!(tmp_file, "# test live layer").unwrap();

    let env_spec = EnvSpec::parse(file_path.display().to_string())
        .expect("absolute path to layer.spfs.yaml should be valid");
    assert!(!env_spec.is_empty());
}
