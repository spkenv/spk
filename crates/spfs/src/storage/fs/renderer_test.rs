// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;

use super::was_render_completed;
use crate::encoding::prelude::*;
use crate::fixtures::*;
use crate::graph::object::{DigestStrategy, EncodingFormat};
use crate::storage::fs::{FsRepository, OpenFsRepository};
use crate::storage::{Repository, RepositoryHandle};
use crate::{tracking, Config};

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
    let mut config = Config::default();
    config.storage.encoding_format = write_encoding_format;
    config.storage.digest_strategy = write_digest_strategy;
    config.make_current().unwrap();

    let storage = OpenFsRepository::create(tmpdir.path().join("storage"))
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
    let mut config = Config::default();
    config.storage.encoding_format = write_encoding_format;
    config.storage.digest_strategy = write_digest_strategy;
    config.make_current().unwrap();

    let tmprepo = Arc::new(
        FsRepository::create(tmpdir.path().join("repo"))
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
        RepositoryHandle::FS(fs) => fs.opened().await.unwrap(),
        _ => panic!("Unexpected tmprepo type!"),
    };

    let render = tmprepo
        .renders
        .as_ref()
        .unwrap()
        .renders
        .build_digest_path(&manifest.digest().unwrap());
    assert!(!render.exists(), "render should NOT be seen as existing");
    super::Renderer::new(&*tmprepo)
        .render_manifest(&manifest, None)
        .await
        .unwrap();
    assert!(render.exists(), "render should be seen as existing");
    assert!(was_render_completed(&render));
    let rendered_manifest = tracking::compute_manifest(&render).await.unwrap();
    let diffs = tracking::compute_diff(&expected_manifest, &rendered_manifest);
    println!("DIFFS:");
    println!("{}", crate::io::format_diffs(diffs.iter()));
    assert_eq!(
        expected_manifest.to_graph_manifest().digest().unwrap(),
        rendered_manifest.to_graph_manifest().digest().unwrap()
    );
}
