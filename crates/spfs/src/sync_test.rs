use rstest::rstest;

use super::{push_ref, sync_ref};
use crate::config::Config;
use crate::prelude::*;
use crate::{encoding, graph, storage, tracking, Error};
use storage::RepositoryHandle;

fixtures!();

#[rstest]
#[tokio::test]
async fn test_push_ref_unknown() {
    let _guard = init_logging();
    match push_ref("--test-unknown--", None).await {
        Err(Error::UnknownReference(_)) => (),
        Err(err) => panic!("expected unknown reference error, got {:?}", err),
        Ok(_) => panic!("expected unknown reference error, got success"),
    }

    match push_ref(encoding::Digest::default().to_string(), None).await {
        Err(Error::UnknownObject(_)) => (),
        Err(err) => panic!("expected unknown reference error, got {:?}", err),
        Ok(_) => panic!("expected unknown reference error, got success"),
    }
}

#[rstest]
#[tokio::test]
async fn test_push_ref(config: (tempdir::TempDir, Config)) {
    let _guard = init_logging();
    let (tmpdir, config) = config;
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let mut local: RepositoryHandle = config.get_repository().unwrap().into();
    let mut remote = config.get_remote("origin").unwrap();
    let manifest = local.commit_dir(src_dir.as_path()).unwrap();
    let layer = local
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    local.push_tag(&tag, &layer.digest().unwrap()).unwrap();

    sync_ref(tag.to_string(), &local, &mut remote)
        .await
        .unwrap();

    assert!(remote.read_ref("testing").is_ok());
    assert!(remote.has_layer(&layer.digest().unwrap()));

    assert!(sync_ref(tag.to_string(), &local, &mut remote).await.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_sync_ref(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let mut repo_a: RepositoryHandle =
        storage::fs::FSRepository::create(tmpdir.path().join("repo_a").as_path())
            .unwrap()
            .into();
    let mut repo_b: RepositoryHandle =
        storage::fs::FSRepository::create(tmpdir.path().join("repo_b").as_path())
            .unwrap()
            .into();

    let manifest = repo_a.commit_dir(src_dir.as_path()).unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a.push_tag(&tag, &platform.digest().unwrap()).unwrap();

    sync_ref("testing", &repo_a, &mut repo_b)
        .await
        .expect("failed to sync ref");

    assert!(repo_b.read_ref("testing").is_ok());
    assert!(repo_b.has_platform(&platform.digest().unwrap()));
    assert!(repo_b.has_layer(&layer.digest().unwrap()));

    std::fs::remove_dir_all(tmpdir.path().join("repo_a")).unwrap();
    std::fs::create_dir_all(tmpdir.path().join("repo_a")).unwrap();
    sync_ref("testing", &repo_b, &mut repo_a).await.unwrap();

    assert!(repo_a.read_ref("testing").is_ok());
    assert!(repo_a.has_layer(&layer.digest().unwrap()));
}

#[rstest]
#[tokio::test]
async fn test_sync_through_tar(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();
    let dir = tmpdir.path();
    let src_dir = dir.join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let mut repo_a: RepositoryHandle = storage::fs::FSRepository::create(dir.join("repo_a"))
        .unwrap()
        .into();
    let mut repo_tar: RepositoryHandle = storage::tar::TarRepository::create(dir.join("repo.tar"))
        .unwrap()
        .into();
    let mut repo_b: RepositoryHandle = storage::fs::FSRepository::create(dir.join("repo_b"))
        .unwrap()
        .into();

    let manifest = repo_a.commit_dir(src_dir.as_path()).unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a.push_tag(&tag, &platform.digest().unwrap()).unwrap();

    sync_ref("testing", &repo_a, &mut repo_tar).await.unwrap();
    drop(repo_tar);
    let repo_tar = storage::tar::TarRepository::open(dir.join("repo.tar"))
        .unwrap()
        .into();
    sync_ref("testing", &repo_tar, &mut repo_b).await.unwrap();

    assert!(repo_b.read_ref("testing").is_ok());
    assert!(repo_b.has_layer(&layer.digest().unwrap()));
}

#[fixture]
fn config(tmpdir: tempdir::TempDir) -> (tempdir::TempDir, Config) {
    let repo_path = tmpdir.path().join("repo");
    crate::storage::fs::FSRepository::create(&repo_path).expect("failed to make repo for test");
    let origin_path = tmpdir.path().join("origin");
    crate::storage::fs::FSRepository::create(&origin_path).expect("failed to make repo for test");
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
