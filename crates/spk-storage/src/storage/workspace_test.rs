// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs;

use rstest::rstest;
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::version;
use spk_workspace::Workspace;
use tempfile::TempDir;

use crate::Repository;
use crate::storage::workspace::WorkspaceRepository;

struct TestWorkspace {
    _temp_dir: TempDir,
    repo: WorkspaceRepository,
}

impl TestWorkspace {
    fn new(files: &[(&str, &str)]) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            fs::write(temp_dir.path().join(name), content).unwrap();
        }
        let workspace = Workspace::builder()
            .load_from_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();
        let repo = WorkspaceRepository::new(
            temp_dir.path(),
            "test-workspace".try_into().unwrap(),
            workspace,
        );
        Self {
            _temp_dir: temp_dir,
            repo,
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_list_package_versions_multiple_templates() {
    let ws = TestWorkspace::new(&[
        (
            "pkg-a.spk.yaml",
            r#"
            pkg: pkg-a
            template:
              versions:
                discover:
                  git_tags:
                    url: https://github.com/spkenv/spk.git
                    match_pattern: "refs/tags/v0.1.*"
                    extract: "refs/tags/v(.*)"
            "#,
        ),
        (
            "pkg-a.v2.spk.yaml",
            r#"
            pkg: pkg-a
            template:
              versions:
                discover:
                  git_tags:
                    url: https://github.com/spkenv/spk.git
                    match_pattern: "refs/tags/v0.2.*"
                    extract: "refs/tags/v(.*)"
            "#,
        ),
    ]);

    let versions = ws
        .repo
        .list_package_versions(&"pkg-a".parse::<PkgNameBuf>().unwrap())
        .await
        .unwrap();
    let versions = versions.iter().map(|v| (**v).clone()).collect::<Vec<_>>();
    assert_eq!(versions, vec![version!("0.1.0"), version!("0.2.0")]);
}

#[rstest]
#[tokio::test]
async fn test_list_package_versions_with_discovery_error() {
    let ws = TestWorkspace::new(&[
        (
            "pkg-a.spk.yaml",
            r#"
            pkg: pkg-a
            template:
              versions:
                discover:
                  git_tags:
                    url: https://github.com/spkenv/spk.git
                    match_pattern: "refs/tags/v0.1.*"
                    extract: "refs/tags/v(.*)"
            "#,
        ),
        (
            "pkg-a.v2.spk.yaml",
            r#"
            pkg: pkg-a
            template:
              versions:
                discover:
                  git_tags:
                    url: https://invalid.url/spkenv/spk.git
                    match_pattern: "refs/tags/v0.2.*"
                    extract: "refs/tags/v(.*)"
            "#,
        ),
    ]);

    let versions = ws
        .repo
        .list_package_versions(&"pkg-a".parse::<PkgNameBuf>().unwrap())
        .await
        .unwrap();
    let versions = versions.iter().map(|v| (**v).clone()).collect::<Vec<_>>();
    assert_eq!(versions, vec![version!("0.1.0")]);
}
