// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::PermissionsExt;

use futures::TryStreamExt;
use rstest::rstest;

use super::{makedirs_with_perms, Data, Storage};
use crate::encoding;
use crate::fixtures::*;

#[rstest]
fn test_config_serialization() {
    let mut expected = Data::new("spfs-testing");
    expected.status.stack = vec![encoding::NULL_DIGEST.into(), encoding::EMPTY_DIGEST.into()];
    let data = serde_json::to_string_pretty(&expected).expect("failed to serialize config");
    let actual: Data = serde_json::from_str(&data).expect("failed to deserialize config data");

    assert_eq!(actual, expected);
}

#[rstest]
#[tokio::test]
async fn test_storage_create_runtime(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime in storage");
    assert!(!runtime.name().is_empty());

    assert!(storage.create_named_runtime(runtime.name()).await.is_err());
}

#[rstest]
#[tokio::test]
async fn test_storage_remove_runtime(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime");
    storage
        .remove_runtime(runtime.name())
        .await
        .expect("should remove runtime properly");
}

#[rstest]
#[tokio::test]
async fn test_storage_iter_runtimes(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let runtimes: Vec<_> = storage
        .iter_runtimes()
        .await
        .try_collect()
        .await
        .expect("unexpected error while listing runtimes");
    assert_eq!(runtimes.len(), 0);

    let _rt1 = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime");
    let runtimes: Vec<_> = storage
        .iter_runtimes()
        .await
        .try_collect()
        .await
        .expect("unexpected error while listing runtimes");
    assert_eq!(runtimes.len(), 1);

    let _rt2 = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime");
    let _rt3 = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime");
    let _rt4 = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime");
    let runtimes: Vec<_> = storage
        .iter_runtimes()
        .await
        .try_collect()
        .await
        .expect("unexpected error while listing runtimes");
    assert_eq!(runtimes.len(), 4);
}

#[rstest]
#[tokio::test]
async fn test_runtime_reset(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime in storage");
    let upper_dir = tmpdir.path().join("upper");
    runtime.data.config.upper_dir = upper_dir.clone();

    ensure(upper_dir.join("file"), "file01");
    ensure(upper_dir.join("dir/file"), "file02");
    ensure(upper_dir.join("dir/dir/dir/file"), "file03");
    ensure(upper_dir.join("dir/dir/dir/file2"), "file04");
    ensure(upper_dir.join("dir/dir/dir1/file"), "file05");
    ensure(upper_dir.join("dir/dir2/dir/file.other"), "other");

    runtime
        .reset(&["file.*"])
        .expect("failed to reset runtime paths");
    assert!(!upper_dir.join("dir/dir2/dir/file.other").exists());
    assert!(upper_dir.join("dir/dir/dir/file2").exists());

    runtime
        .reset(&["dir1/"])
        .expect("failed to reset runtime paths");
    assert!(upper_dir.join("dir/dir/dir").exists());
    assert!(upper_dir.join("dir/dir2").exists());

    runtime
        .reset(&["/file"])
        .expect("failed to reset runtime paths");
    assert!(upper_dir.join("dir/dir/dir/file").exists());
    assert!(!upper_dir.join("file").exists());

    runtime.reset_all().expect("failed to reset runtime paths");
    assert_eq!(listdir(upper_dir), Vec::<String>::new());
}

#[rstest]
fn test_makedirs_dont_change_existing(tmpdir: tempfile::TempDir) {
    let chkdir = tmpdir.path().join("my_dir");
    ensure(chkdir.join("file"), "data");
    std::fs::set_permissions(&chkdir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let original = std::fs::metadata(&chkdir).unwrap().permissions().mode();
    makedirs_with_perms(chkdir.join("new"), 0o777).expect("makedirs should not fail");
    let actual = std::fs::metadata(&chkdir).unwrap().permissions().mode();
    assert_eq!(actual, original, "existing dir should not change perms");
}

fn listdir(path: std::path::PathBuf) -> Vec<String> {
    std::fs::read_dir(path)
        .expect("failed to read dir")
        .map(|res| {
            res.expect("error while reading dir")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect()
}
