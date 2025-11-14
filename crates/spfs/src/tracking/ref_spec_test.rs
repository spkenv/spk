// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs::File;
use std::io::Write;

use rstest::rstest;

use super::RefSpec;
use crate::fixtures::tmpdir;
use crate::tracking;

#[rstest]
fn test_ref_spec_validation() {
    let spec = RefSpec::parse("one+two").expect("failed to parse ref spec");
    assert_eq!(spec.items.len(), 2);
}

#[rstest]
fn test_ref_spec_empty() {
    let _ = RefSpec::parse("").expect_err("empty spec should be invalid");
    let _ = RefSpec::parse(tracking::ENV_SPEC_EMPTY).expect_err("dash spec should be invalid");
}

#[rstest]
fn test_ref_spec_with_live_layer_dir(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("layer.spfs.yaml");
    let mut tmp_file = File::create(file_path).unwrap();
    writeln!(tmp_file, "# test live layer").unwrap();

    let ref_spec = RefSpec::parse(dir.display().to_string())
        .expect("absolute directory containing a layer.spfs.yaml should be valid");
    assert!(!ref_spec.is_empty())
}

#[rstest]
fn test_ref_spec_with_live_layer_file(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("livelayer.spfs.yaml");
    let mut tmp_file = File::create(file_path.clone()).unwrap();
    writeln!(tmp_file, "# test live layer").unwrap();

    let ref_spec = RefSpec::parse(file_path.display().to_string())
        .expect("absolute path to livelayer.spfs.yaml should be valid");
    assert!(!ref_spec.is_empty());
}

#[rstest]
fn test_ref_spec_with_runspec_file(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("runspec.spfs.yaml");
    let mut tmp_file = File::create(file_path.clone()).unwrap();
    writeln!(
        tmp_file,
        "# test run spec\napi: spfs/v0/runspec\nlayers:\n
  - A7USTIBXPXHMD5CYEIIOBMFLM3X77ESVR3WAUXQ7XQQGTHKH7DMQ===="
    )
    .unwrap();

    let ref_spec = RefSpec::parse(file_path.display().to_string())
        .expect("absolute path to runspec.spfs.yaml should be valid");
    assert!(!ref_spec.is_empty());
}

#[rstest]
fn test_ref_spec_with_empty_runspec_file(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();
    let file_path = dir.join("runspec.spfs.yaml");
    let mut tmp_file = File::create(file_path.clone()).unwrap();
    writeln!(
        tmp_file,
        "# test run spec\napi: spfs/v0/runspec\nlayers: []\n"
    )
    .unwrap();

    let _ = RefSpec::parse(file_path.display().to_string())
        .expect_err("empty runspec.spfs.yaml should be invalid");
}
