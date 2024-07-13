// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spfs::storage::{LayerStorage, ManifestStorage, PayloadStorage};
use spk_cli_common::BuildArtifact;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident_component::Component;
use spk_storage::fixtures::*;

use crate::try_build_package;

#[rstest]
#[case::simple(
    "- { set: TEST_VAR, value: \"${SPK_PKG_VERSION_MAJOR}\" }",
    "export TEST_VAR=\"1\"\n"
)]
#[case::unexpanded_and_expanded(
    "- { set: TEST_VAR, value: \"$$HOME/${SPK_PKG_VERSION_MAJOR}.${SPK_PKG_VERSION_MINOR}\" }",
    "export TEST_VAR=\"$HOME/1.2\"\n"
)]
#[tokio::test]
async fn basic_environment_generation(
    tmpdir: tempfile::TempDir,
    #[case] env_spec: &str,
    #[case] expected: &str,
) {
    let rt = spfs_runtime().await;

    let (_, result) = try_build_package!(
        tmpdir,
        "test.spk.yaml",
        format!(
            r#"
pkg: test/1.2.3
api: v0/package
build:
    script:
        - true
install:
    environment:
        {env_spec}
        "#
        ),
    );

    let mut result = result.expect("Expected build to succeed");

    // Only care about binary builds (not source builds)
    result
        .created_builds
        .artifacts
        .retain(|(_, artifact)| matches!(artifact, BuildArtifact::Binary(_, _, _)));

    assert_eq!(
        result.created_builds.artifacts.len(),
        1,
        "Expected one build to be created"
    );

    // Check the generated activation script

    let BuildArtifact::Binary(build, _, _) = &result.created_builds.artifacts[0].1 else {
        panic!("Expected binary build");
    };

    let digest = *rt
        .tmprepo
        .read_components(build)
        .await
        .unwrap()
        .get(&Component::Run)
        .unwrap();

    let spk_storage::RepositoryHandle::SPFS(repo) = &*rt.tmprepo else {
        panic!("Expected SPFS repo");
    };

    let layer = repo.read_layer(digest).await.unwrap();

    let manifest = repo
        .read_manifest(
            *layer
                .manifest()
                .expect("Layer should have a manifest in this test"),
        )
        .await
        .unwrap()
        .to_tracking_manifest();

    let entry = manifest.get_path("etc/spfs/startup.d/spk_test.sh").unwrap();

    let (mut payload, _filename) = repo.open_payload(entry.object).await.unwrap();
    let mut writer: Vec<u8> = vec![];
    tokio::io::copy(&mut payload, &mut writer).await.unwrap();
    let contents = String::from_utf8(writer).unwrap();
    assert_eq!(contents, expected);
}
