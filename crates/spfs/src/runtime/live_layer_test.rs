// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use rstest::rstest;

use crate::fixtures::*;
use crate::runtime::{BindMount, LiveLayer, SpecApiVersion};
use crate::tracking::SpecFile;

#[rstest]
fn test_bindmount_creation() {
    let dir = "/some/dir/some/where";
    let mountpoint = "tests/tests/tests".to_string();
    let expected = format!("{dir}:{mountpoint}");

    let mount = BindMount {
        src: PathBuf::from(dir),
        dest: mountpoint,
    };

    assert_eq!(mount.to_string(), expected);
}

#[rstest]
fn test_bindmount_validate(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let mount = BindMount {
        src: subdir,
        dest: mountpoint,
    };

    assert!(mount.validate(path.to_path_buf()).is_ok());
}

#[rstest]
fn test_bindmount_validate_fail_not_under_parent(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let mount = BindMount {
        src: subdir,
        dest: mountpoint,
    };

    assert!(mount
        .validate(PathBuf::from_str("/tmp/no/its/parent/").unwrap())
        .is_err());
}

#[rstest]
fn test_bindmount_validate_fail_not_exists(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let missing_subdir = subdir.join("not_made");

    let mount = BindMount {
        src: missing_subdir,
        dest: mountpoint,
    };

    assert!(mount.validate(path.to_path_buf()).is_err());
}

#[rstest]
fn test_live_layer_file_load(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();

    let subdir = dir.join("testing");
    std::fs::create_dir(subdir.clone()).unwrap();

    let yaml = format!(
        "# test live layer\napi: v0/layer\ncontents:\n - bind: {}\n   dest: /spfs/test\n",
        subdir.display()
    );

    let file_path = dir.join("layer.spfs.yaml");
    let mut tmp_file = File::create(file_path).unwrap();
    writeln!(tmp_file, "{}", yaml).unwrap();

    let ll = SpecFile::parse(&dir.display().to_string()).unwrap();

    if let SpecFile::LiveLayer(live_layer) = ll {
        assert!(live_layer.api == SpecApiVersion::V0Layer);
        assert!(!live_layer.contents.is_empty());
        assert!(live_layer.contents.len() == 1);
    } else {
        panic!("The test yaml should have parsed as a live layer, but it didn't")
    }
}

#[rstest]
fn test_live_layer_minimal_deserialize() {
    // Test a minimal yaml string that represents a LiveLayer. Note:
    // if more LiveLayer fields are added in future, they should have
    // #[serde(default)] set or be optional, so they are backwards
    // compatible with existing live layer configurations.
    let yaml: &str = "api: v0/layer\ncontents:\n";

    let layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();

    assert!(layer.api == SpecApiVersion::V0Layer);
}

#[rstest]
#[should_panic]
fn test_live_layer_deserialize_fail_no_contents_field() {
    let yaml: &str = "api: v0/layer\n";

    // This should panic because the contents: field is missing
    let _layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();
}

#[rstest]
#[should_panic]
fn test_live_layer_deserialize_unknown_version() {
    let yaml: &str = "api: v9999999999999/invalidapi\ncontents:\n";

    // This should panic because the api value is invalid
    let _layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();
}
