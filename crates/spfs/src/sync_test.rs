use rstest::{fixture, rstest};

use super::{push_ref, sync_ref};
use crate::config::Config;
use crate::prelude::*;
use crate::{encoding, graph, storage, tracking, Error};

#[rstest]
#[tokio::test]
async fn test_push_ref_unknown() {
    if let Error::UnknownReference(_) = push_ref("--test-unknown--", None) {
        // ok
    } else {
        panic!("expected unknown reference error");
    }

    if let Error::UnknownReference(_) = push_ref(str(encoding::NULL_DIGEST), None) {
        // ok
    } else {
        panic!("expected unknown reference error");
    }
}

#[rstest]
#[tokio::test]
async fn test_push_ref(config: Config, tmpdir: tempdir::TempDir) {
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let local = config.get_repository().unwrap();
    let remote = config.get_remote("origin").unwrap();
    let manifest = local.commit_dir(src_dir.as_path()).unwrap();
    let layer = local
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    local.push_tag(&tag, &layer.digest().unwrap()).unwrap();

    push_ref(&tag, None).unwrap();

    assert!(remote.read_ref("testing").is_ok());
    assert!(remote.has_layer(layer.digest().unwrap()));

    assert!(push_ref(&tag, None).is_ok());
}

#[rstest]
#[tokio::test]
async fn test_sync_ref(tmpdir: tempdir::TempDir) {
    let src_dir = tmpdir.path().join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let repo_a = storage::fs::FSRepository::create(tmpdir.path().join("repo_a")).unwrap();
    let repo_b = storage::fs::FSRepository::create(tmpdir.path().join("repo_b")).unwrap();

    let manifest = repo_a.commit_dir(src_dir).unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a.push_tag(&tag, &platform.digest().unwrap()).unwrap();

    sync_ref("testing", repo_a, repo_b)
        .await
        .expect("failed to sync ref");

    assert!(repo_b.read_ref("testing").is_ok());
    assert!(repo_b.has_platform(platform.digest().unwrap()));
    assert!(repo_b.has_layer(layer.digest().unwrap()));

    std::fs::remove_dir_all(tmpdir.path().join("repo_a")).unwrap();
    std::fs::create_dir_all(tmpdir.path().join("repo_a")).unwrap();
    sync_ref("testing", repo_b, repo_a).await.unwrap();

    assert!(repo_a.read_ref("testing").is_ok());
    assert!(repo_a.has_layer(layer.digest().unwrap()));
}

#[rstest]
#[tokio::test]
async fn test_sync_through_tar(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let src_dir = tmpdir.join("source");
    ensure(src_dir.join("dir/file.txt"), "hello");
    ensure(src_dir.join("dir2/otherfile.txt"), "hello2");
    ensure(src_dir.join("dir//dir/dir/file.txt"), "hello, world");

    let repo_a = storage::fs::FSRepository::create(tmpdir.join("repo_a")).unwrap();
    let repo_tar = storage::tar::TarRepository::create(tmpdir.join("repo.tar")).unwrap();
    let repo_b = storage::fs::FSRepository::create(tmpdir.join("repo_b")).unwrap();

    let manifest = repo_a.commit_dir(src_dir.as_path()).unwrap();
    let layer = repo_a
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let platform = repo_a
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();
    let tag = tracking::TagSpec::parse("testing").unwrap();
    repo_a.push_tag(&tag, platform.digest()).unwrap();

    sync_ref("testing", repo_a, repo_tar).await.unwrap();
    let repo_tar = storage::tar::TarRepository::open(tmpdir.join("repo.tar")).unwrap();
    sync_ref("testing", repo_tar, repo_b).await.unwrap();

    assert!(repo_b.read_ref("testing"));
    assert!(repo_b.has_layer(layer.digest()));
}

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-sync-test").unwrap()
}

#[fixture]
fn config(tmpdir: tempdir::TempDir) -> Config {
    let repo_path = tmpdir.path().join("repo");
    let mut conf = Config::default();
    conf.storage.root = repo_path;
    conf
}

fn ensure(path: std::path::PathBuf, data: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("failed to make dirs");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .expect("failed to create file");
    std::io::copy(&mut data.as_bytes(), &mut file).expect("failed to write file data");
}
