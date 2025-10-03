// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs;

use rstest::rstest;
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::ident_build::{Build, BuildId};
use spk_schema::prelude::*;
use spk_schema::{VersionIdent, version};
use spk_workspace::Workspace;
use tempfile::TempDir;

use crate::storage::workspace::WorkspaceRepository;
use crate::{Repository, Storage};

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
            .with_root(temp_dir.path())
            .with_ignore_invalid_files(false)
            .with_glob_pattern("*.spk.yaml")
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
            template:
              versions:
                static: ["0.1.0"]
            pkg: pkg-a/{{ version }}
            "#,
        ),
        (
            "pkg-a.v2.spk.yaml",
            r#"
            template:
              versions:
                static: ["0.2.0"]
            pkg: pkg-a/{{ version }}
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
                static: ["0.1.0"]
            "#,
        ),
        (
            "pkg-a.v2.spk.yaml",
            r#"
            pkg: pkg-a
            template:
              versions:
                discover:
                  gitTags: "v0.2.*"
                  url: https://invalid.url/spkenv/spk.git
                  extract: "v(.*)"
            "#,
        ),
    ]);

    ws.repo
        .list_package_versions(&"pkg-a".parse::<PkgNameBuf>().unwrap())
        .await
        .expect_err("should fail when discovery is not possible");
}

#[rstest]
#[tokio::test]
async fn test_get_concrete_package_builds() {
    let ws = TestWorkspace::new(&[
        (
            "pkg-a.spk.yaml",
            r#"
            pkg: pkg-a
            version: 1.0.0
            "#,
        ),
        (
            "pkg-b.spk.yaml",
            r#"
            pkg: pkg-b
            version: 2.0.0
            "#,
        ),
    ]);
    let ident = VersionIdent::new("pkg-a".parse().unwrap(), version!("1.0.0"));
    let builds = ws.repo.get_concrete_package_builds(&ident).await.unwrap();
    assert_eq!(builds.len(), 1);
    assert!(builds.contains(&ident.to_build_ident(Build::Source)));
}

#[rstest]
#[tokio::test]
async fn test_read_package_from_storage() {
    let ws = TestWorkspace::new(&[(
        "pkg-a.spk.yaml",
        r#"
        template:
          versions:
            static: ["1.0.0"]
        pkg: pkg-a/{{ version }}
        "#,
    )]);
    let ident = "pkg-a/1.0.0/src".parse().unwrap();
    let build = ws.repo.read_package_from_storage(&ident).await.unwrap();
    assert_eq!(build.ident(), &ident);
}

#[rstest]
#[tokio::test]
async fn test_read_package_from_storage_not_found() {
    let ws = TestWorkspace::new(&[(
        "pkg-a.spk.yaml",
        r#"
        pkg: pkg-a
        template:
          versions:
            static: ["1.0.0"]
        "#,
    )]);
    let ident = "pkg-a/2.0.0/src".parse().unwrap();
    let err = ws.repo.read_package_from_storage(&ident).await.unwrap_err();
    assert!(matches!(err, crate::Error::PackageNotFound(_)));
}

#[rstest]
#[tokio::test]
async fn test_read_package_from_storage_not_source() {
    let ws = TestWorkspace::new(&[(
        "pkg-a.spk.yaml",
        r#"
        pkg: pkg-a
        template:
          versions:
            static: ["1.0.0"]
        "#,
    )]);
    let ident = VersionIdent::new("pkg-a".parse().unwrap(), version!("1.0.0"))
        .to_build_ident(Build::BuildId(BuildId::default()));
    let err = ws.repo.read_package_from_storage(&ident).await.unwrap_err();
    assert!(matches!(err, crate::Error::PackageNotFound(_)));
}

#[rstest]
#[tokio::test]
async fn test_list_build_components() {
    let ws = TestWorkspace::new(&[(
        "pkg-a.spk.yaml",
        r#"
        pkg: pkg-a
        template:
          versions:
            static: ["1.0.0"]
        "#,
    )]);
    let ident = "pkg-a/1.0.0/src".parse().unwrap();
    let components = ws.repo.list_build_components(&ident).await.unwrap();
    assert_eq!(
        components,
        vec![spk_schema::foundation::ident_component::Component::Source]
    );

    let ident = VersionIdent::new("pkg-a".parse().unwrap(), version!("1.0.0"))
        .to_build_ident(Build::BuildId(BuildId::default()));
    let err = ws.repo.list_build_components(&ident).await.unwrap_err();
    assert!(matches!(err, crate::Error::PackageNotFound(_)));
}

#[rstest]
#[tokio::test]
async fn test_read_recipe() {
    let ws = TestWorkspace::new(&[(
        "pkg-a.spk.yaml",
        r#"
        pkg: pkg-a/{{ version }}
        template:
          versions:
            static: ["1.0.0"]
        "#,
    )]);
    let ident = "pkg-a/1.0.0".parse().unwrap();
    let recipe = ws.repo.read_recipe(&ident).await.unwrap();
    assert_eq!(recipe.ident(), &ident);

    let ident = "pkg-a/2.0.0".parse().unwrap();
    let err = ws.repo.read_recipe(&ident).await.unwrap_err();
    assert!(matches!(err, crate::Error::PackageNotFound(_)));
}
