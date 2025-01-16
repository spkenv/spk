// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::{fixture, rstest};

use super::WorkspaceFile;

#[fixture]
pub fn tmpdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("spk-test-")
        .tempdir()
        .expect("create a temp directory for test files")
}

const EMPTY_WORKSPACE: &str = r#"
api: v0/workspace
recipes: []
"#;

#[rstest]
#[case(
    r#"
api: v0/workspace
recipes:
  - packages/**/*.spk.yaml
  - path: packages/python/python2.spk.yaml
    versions: [2.7.18]
  - path: packages/python/python3.spk.yaml
    versions:
      - '3.7.{0..17}'
      - '3.8.{0..20}'
      - '3.9.{0..21}'
"#
)]
fn test_workspace_from_yaml(#[case] yaml: &str) {
    let _deserialized: WorkspaceFile = serde_yaml::from_str(yaml).unwrap();
}

#[rstest]
fn test_empty_workspace_loading(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path();
    std::fs::write(root.join(WorkspaceFile::FILE_NAME), EMPTY_WORKSPACE).unwrap();
    let _workspace = WorkspaceFile::load(root).expect("failed to load empty workspace");
}

#[rstest]
fn test_must_have_file(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path();
    WorkspaceFile::load(root).expect_err("workspace should fail to load for empty dir");
}

#[rstest]
#[case("", "")]
#[case("my-workspace", "my-workspace/src/packages")]
#[should_panic]
#[case("my-workspace", "other-dir")]
#[should_panic]
#[case("my-workspace", "other-dir/src/packages")]
fn test_workspace_discovery(
    tmpdir: tempfile::TempDir,
    #[case] workspace_root: &str,
    #[case] discovery_start: &str,
) {
    let dir = tmpdir.path();
    let root = dir.join(workspace_root);
    let cwd = dir.join(discovery_start);

    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(WorkspaceFile::FILE_NAME), EMPTY_WORKSPACE).unwrap();

    WorkspaceFile::discover(&cwd).expect("failed to load workspace");
}
