// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::{fixture, rstest};

use super::{push_ref, sync_ref};
use crate::config::Config;
use crate::prelude::*;
use crate::{encoding, graph, storage, tracking, Error};
use storage::RepositoryHandle;

use crate::fixtures::*;

#[rstest]
#[tokio::test]
async fn test_push_ref_unknown(#[future] config: (tempdir::TempDir, Config)) {
    init_logging();
    let (_handle, config) = config.await;
    match push_ref(
        "--test-unknown--",
        Some(config.get_remote("origin").await.unwrap()),
    )
    .await
    {
        Err(Error::UnknownReference(_)) => (),
        Err(err) => panic!("expected unknown reference error, got {:?}", err),
        Ok(_) => panic!("expected unknown reference error, got success"),
    }

    match push_ref(
        encoding::Digest::default().to_string(),
        Some(config.get_remote("origin").await.unwrap()),
    )
    .await
    {
        Err(Error::UnknownObject(_)) => (),
        Err(err) => panic!("expected unknown object error, got {:?}", err),
        Ok(_) => panic!("expected unknown object error, got success"),
    }
}

#[rstest]
#[tokio::test]
async fn test_push_ref(#[future] config: (tempdir::TempDir, Config)) {
    init_logging();
    let (tmpdir, config) = config.await;
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let local: RepositoryHandle = config.get_repository().await.unwrap().into();
    let remote = config.get_remote("origin").await.unwrap();
    let manifest = local.commit_dir(src_dir.as_path()).await.unwrap();
    let layer = local
        .create_layer(&graph::Manifest::from(&manifest))
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    local
        .push_tag(&tag, &layer.digest().unwrap())
        .await
        .unwrap();

    sync_ref(tag.to_string(), &local, &remote).await.unwrap();

    assert!(remote.read_ref("testing").await.is_ok());
    assert!(remote.has_layer(layer.digest().unwrap()).await);

    assert!(sync_ref(tag.to_string(), &local, &remote).await.is_ok());
}

#[rstest(
    repo_a,
    repo_b,
    case::fs(tmprepo("fs"), tmprepo("fs")),
    case::tar(tmprepo("tar"), tmprepo("tar"))
)]
#[tokio::test]
async fn test_sync_ref(#[future] repo_a: TempRepo, #[future] repo_b: TempRepo, tmpdir: tempdir::TempDir) {
    init_logging();
    let repo_a = repo_a.await;
    let repo_b = repo_b.await;

    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let manifest = repo_a.commit_dir(src_dir.as_path()).await.unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .await
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a
        .push_tag(&tag, &platform.digest().unwrap())
        .await
        .unwrap();

    sync_ref("testing", &repo_a, &repo_b)
        .await
        .expect("failed to sync ref");

    assert!(repo_b.read_ref("testing").await.is_ok());
    assert!(repo_b.has_platform(platform.digest().unwrap()).await);
    assert!(repo_b.has_layer(layer.digest().unwrap()).await);

    sync_ref("testing", &repo_b, &repo_a)
        .await
        .expect("failed to sync back");

    assert!(repo_a.read_ref("testing").await.is_ok());
    assert!(repo_a.has_layer(layer.digest().unwrap()).await);
}

#[rstest(
    repo_a,
    repo_b,
    case::fs(tmprepo("fs"), tmprepo("fs")),
    case::tar(tmprepo("tar"), tmprepo("tar"))
)]
#[tokio::test]
async fn test_sync_through_tar(#[future] repo_a: TempRepo, #[future] repo_b: TempRepo, tmpdir: tempdir::TempDir) {
    init_logging();
    let repo_a = repo_a.await;
    let repo_b = repo_b.await;

    let dir = tmpdir.path();
    let src_dir = dir.join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let repo_tar: RepositoryHandle = storage::tar::TarRepository::create(dir.join("repo.tar"))
        .await
        .unwrap()
        .into();

    let manifest = repo_a.commit_dir(src_dir.as_path()).await.unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .await
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .await
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a
        .push_tag(&tag, &platform.digest().unwrap())
        .await
        .unwrap();

    sync_ref("testing", &repo_a, &repo_tar).await.unwrap();
    drop(repo_tar);
    let repo_tar = storage::tar::TarRepository::open(dir.join("repo.tar"))
        .await
        .unwrap()
        .into();
    sync_ref("testing", &repo_tar, &repo_b).await.unwrap();

    assert!(repo_b.read_ref("testing").await.is_ok());
    assert!(repo_b.has_layer(layer.digest().unwrap()).await);
}

#[fixture]
async fn config(tmpdir: tempdir::TempDir) -> (tempdir::TempDir, Config) {
    let repo_path = tmpdir.path().join("repo");
    crate::storage::fs::FSRepository::create(&repo_path)
        .await
        .expect("failed to make repo for test");
    let origin_path = tmpdir.path().join("origin");
    crate::storage::fs::FSRepository::create(&origin_path)
        .await
        .expect("failed to make repo for test");
    let mut conf = Config::default();
    conf.remote.insert(
        "origin".to_string(),
        crate::config::Remote {
            address: url::Url::from_file_path(&origin_path).unwrap(),
        },
    );
    conf.storage.root = repo_path;
    (tmpdir, conf)
}
