// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use futures::TryStreamExt;
use rstest::rstest;

use super::{makedirs_with_perms, Data, Storage};
use crate::encoding;
use crate::fixtures::*;
use crate::runtime::storage::{LiveLayerApiVersion, LiveLayerContents};
use crate::runtime::{BindMount, LiveLayer, LiveLayerFile};

#[rstest]
fn test_bindmount_creation() {
    let dir = "/some/dir/some/where";
    let mountpoint = "tests/tests/tests".to_string();
    let expected = format!("{dir}:{mountpoint}");

    let mount = BindMount {
        src: PathBuf::from(dir),
        dest: mountpoint,
    };

    assert_eq!(mount.to_string(), expected);
}

#[rstest]
fn test_bindmount_validate(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let mount = BindMount {
        src: subdir,
        dest: mountpoint,
    };

    assert!(mount.validate(path.to_path_buf()).is_ok());
}

#[rstest]
fn test_bindmount_validate_fail_not_under_parent(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let mount = BindMount {
        src: subdir,
        dest: mountpoint,
    };

    assert!(mount
        .validate(PathBuf::from_str("/tmp/no/its/parent/").unwrap())
        .is_err());
}

#[rstest]
fn test_bindmount_validate_fail_not_exists(tmpdir: tempfile::TempDir) {
    let path = tmpdir.path();
    let subdir = path.join("somedir");
    std::fs::create_dir(subdir.clone()).unwrap();

    let mountpoint = "tests/tests/tests".to_string();

    let missing_subdir = subdir.join("not_made");

    let mount = BindMount {
        src: missing_subdir,
        dest: mountpoint,
    };

    assert!(mount.validate(path.to_path_buf()).is_err());
}

#[rstest]
fn test_live_layer_file_load(tmpdir: tempfile::TempDir) {
    let dir = tmpdir.path();

    let subdir = dir.join("testing");
    std::fs::create_dir(subdir.clone()).unwrap();

    let yaml = format!(
        "# test live layer\napi: v0/layer\ncontents:\n - bind: {}\n   dest: /spfs/test\n",
        subdir.display()
    );

    let file_path = dir.join("layer.spfs.yaml");
    let mut tmp_file = File::create(file_path).unwrap();
    writeln!(tmp_file, "{}", yaml).unwrap();

    let llf = LiveLayerFile::parse(dir.display().to_string()).unwrap();

    let live_layer = llf.load();
    assert!(live_layer.is_ok());
}

#[rstest]
fn test_live_layer_minimal_deserialize() {
    // Test a minimal yaml string that represents a LiveLayer. Note:
    // if more LiveLayer fields are added in future, they should have
    // #[serde(default)] set or be optional, so they are backwards
    // compatible with existing live layer configurations.
    let yaml: &str = "api: v0/layer\ncontents:\n";

    let layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();

    assert!(layer.api == LiveLayerApiVersion::V0Layer);
}

#[rstest]
#[should_panic]
fn test_live_layer_deserialize_fail_no_contents_field() {
    let yaml: &str = "api: v0/layer\n";

    // This should panic because the contents: field is missing
    let _layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();
}

#[rstest]
#[should_panic]
fn test_live_layer_deserialize_unknown_version() {
    let yaml: &str = "api: v9999999999999/invalidapi\ncontents:\n";

    // This should panic because the api value is invalid
    let _layer: LiveLayer = serde_yaml::from_str(yaml).unwrap();
}

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
        crate::storage::fs::FsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime in storage");
    assert!(!runtime.name().is_empty());

    let durable = false;
    let live_layers = Vec::new();
    assert!(storage
        .create_named_runtime(runtime.name(), durable, live_layers)
        .await
        .is_err());
}

#[rstest]
#[tokio::test]
async fn test_storage_remove_runtime(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FsRepository::create(root)
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
        crate::storage::fs::FsRepository::create(root)
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
        crate::storage::fs::FsRepository::create(root)
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
#[tokio::test]
async fn test_runtime_ensure_extra_bind_mount_locations_exist(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo);

    let dir = "/tmp";
    let mountpoint = "tests/tests/tests".to_string();
    let mount = BindMount {
        src: dir.into(),
        dest: mountpoint,
    };
    let live_layer = LiveLayer {
        api: LiveLayerApiVersion::V0Layer,
        contents: vec![LiveLayerContents::BindMount(mount)],
    };
    let live_layers = vec![live_layer];

    let keep_runtime = false;
    let mut runtime = storage
        .create_runtime(keep_runtime, live_layers)
        .await
        .expect("failed to create runtime in storage");

    let layers = runtime.live_layers();

    if !layers.is_empty() {
        assert!(layers.len() == 1)
    } else {
        panic!("a live layer should have been added to the runtime")
    };

    assert!(runtime.prepare_live_layers().await.is_ok())
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
