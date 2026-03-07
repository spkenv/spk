// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;

use super::was_render_completed;
use crate::encoding::prelude::*;
use crate::fixtures::*;
use crate::graph::object::{DigestStrategy, EncodingFormat};
use crate::storage::fs::{MaybeOpenFsRepository, NoRenderStore, OpenFsRepository, RenderStore};
use crate::storage::{LayerStorageExt, RepositoryExt, RepositoryHandle, TagStorage};
use crate::{Config, reset_config_async, tracking};

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_render_manifest(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let storage = OpenFsRepository::<RenderStore>::create(tmpdir.path().join("storage"))
            .await
            .unwrap();

        let src_dir = tmpdir.path().join("source");
        ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
        ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
        ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
        ensure(src_dir.join("file.txt"), "rootdata");

        let manifest = tracking::compute_manifest(&src_dir).await.unwrap();

        for node in manifest.walk_abs(src_dir.to_str().unwrap()) {
            if node.entry.kind.is_blob() {
                let data = tokio::fs::File::open(&node.path.to_path("/"))
                    .await
                    .unwrap();
                storage
                    .commit_blob(Box::pin(tokio::io::BufReader::new(data)))
                    .await
                    .unwrap();
            }
        }

        let expected = manifest.to_graph_manifest();
        let rendered_path = crate::storage::fs::Renderer::new(&storage)
            .render_manifest(&expected, None)
            .await
            .expect("should successfully render manifest");
        let actual = tracking::compute_manifest(rendered_path)
            .await
            .unwrap()
            .to_graph_manifest();
        assert_eq!(actual.digest().unwrap(), expected.digest().unwrap());
    }
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_render_manifest_with_repo(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let tmprepo = Arc::new(
            MaybeOpenFsRepository::<RenderStore>::create(tmpdir.path().join("repo"))
                .await
                .unwrap()
                .into(),
        );
        let src_dir = tmpdir.path().join("source");
        ensure(src_dir.join("dir1.0/dir2.0/file.txt"), "somedata");
        ensure(src_dir.join("dir1.0/dir2.1/file.txt"), "someotherdata");
        ensure(src_dir.join("dir2.0/file.txt"), "evenmoredata");
        ensure(src_dir.join("file.txt"), "rootdata");

        let expected_manifest = crate::Committer::new(&tmprepo)
            .commit_dir(&src_dir)
            .await
            .unwrap();
        let manifest = expected_manifest.to_graph_manifest();

        // Safety: tmprepo was created as an FsRepository
        let tmprepo = match &*tmprepo {
            RepositoryHandle::FSWithRenders(fs) => fs.opened().await.unwrap(),
            _ => panic!("Unexpected tmprepo type!"),
        };

        let render = tmprepo
            .fs_impl
            .rs_impl
            .renders
            .build_digest_path(&manifest.digest().unwrap());
        assert!(!render.exists(), "render should NOT be seen as existing");
        super::Renderer::new(&tmprepo)
            .render_manifest(&manifest, None)
            .await
            .unwrap();
        assert!(render.exists(), "render should be seen as existing");
        assert!(was_render_completed(&render).await);
        let rendered_manifest = tracking::compute_manifest(&render).await.unwrap();
        let diffs = tracking::compute_diff(&expected_manifest, &rendered_manifest);
        println!("DIFFS:");
        println!("{}", crate::io::format_diffs(diffs.iter()));
        assert_eq!(
            expected_manifest.to_graph_manifest().digest().unwrap(),
            rendered_manifest.to_graph_manifest().digest().unwrap()
        );
    }
}

#[tokio::test]
async fn test_render_into_directory_without_render_store_does_not_create_renders_dir() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let repo_root = tmpdir.path().join("repo");
    let repo = MaybeOpenFsRepository::<NoRenderStore>::create(&repo_root)
        .await
        .unwrap();
    let repo_handle = Arc::new(RepositoryHandle::from(repo.clone()));

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("file.txt"), "world");
    let expected_manifest = crate::Committer::new(&repo_handle)
        .commit_dir(&src_dir)
        .await
        .unwrap();
    let layer = repo_handle
        .create_layer(&expected_manifest.to_graph_manifest())
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("renderer/no-render-store").unwrap();
    repo_handle
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();

    let opened = repo.opened().await.unwrap();
    let target_dir = tmpdir.path().join("rendered");
    let env_spec = tracking::EnvSpec::parse(tag.to_string()).unwrap();
    super::Renderer::new(&opened)
        .render_into_directory(
            env_spec,
            &target_dir,
            super::RenderType::HardLink(super::HardLinkRenderType::WithoutProxy),
        )
        .await
        .unwrap();

    let root_contents = tokio::fs::read_to_string(target_dir.join("file.txt"))
        .await
        .unwrap();
    let nested_contents = tokio::fs::read_to_string(target_dir.join("dir/file.txt"))
        .await
        .unwrap();
    assert_eq!(root_contents, "world");
    assert_eq!(nested_contents, "hello");
    assert!(
        !repo_root.join("renders").exists(),
        "render_into_directory should not create repository renders dir"
    );
}
