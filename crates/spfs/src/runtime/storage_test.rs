// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use futures::TryStreamExt;
use rstest::rstest;
use spfs_encoding::Digestible;

#[cfg(unix)]
use super::makedirs_with_perms;
use super::{Data, Storage};
use crate::fixtures::*;
use crate::graph::object::{DigestStrategy, EncodingFormat};
use crate::graph::{AnnotationValue, Layer, Platform};
use crate::runtime::{BindMount, KeyValuePair, LiveLayer, LiveLayerContents, SpecApiVersion};
use crate::storage::prelude::DatabaseExt;
use crate::{Config, encoding, reset_config_async};

#[rstest]
fn test_config_serialization() {
    let mut expected = Data::new("spfs-testing");
    expected.status.stack.push(encoding::NULL_DIGEST.into());
    expected.status.stack.push(encoding::EMPTY_DIGEST.into());
    let data = serde_json::to_string_pretty(&expected).expect("failed to serialize config");
    let actual: Data = serde_json::from_str(&data).expect("failed to deserialize config data");

    // The mount_backend field is skipped during serialization when it's OverlayFsWithRenders
    // (for backward compatibility with older configs). On deserialization, it defaults to
    // MountBackend::default() which is platform-specific. This is intentional - configs
    // without an explicit mount_backend should use the platform-appropriate default.
    // For this test, we just need to ensure the deserialized value uses the platform default.
    let mut expected_with_platform_default = expected.clone();
    expected_with_platform_default.config.mount_backend = crate::runtime::MountBackend::default();
    assert_eq!(actual, expected_with_platform_default);
}

#[rstest]
#[tokio::test]
async fn test_storage_create_runtime(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::MaybeOpenFsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo).unwrap();

    let runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime in storage");
    assert!(!runtime.name().is_empty());

    let durable = false;
    let live_layers = Vec::new();
    assert!(
        storage
            .create_named_runtime(runtime.name(), durable, live_layers)
            .await
            .is_err()
    );
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_storage_runtime_with_annotation(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let root = tmpdir.path().to_string_lossy().to_string();
        let repo = crate::storage::RepositoryHandle::from(
            crate::storage::fs::MaybeOpenFsRepository::create(root)
                .await
                .unwrap(),
        );
        let storage = Storage::new(repo).unwrap();
        let limit: usize = 16 * 1024;

        let keep_runtime = false;
        let live_layers = Vec::new();
        let mut runtime = storage
            .create_named_runtime("test-with-annotation-data", keep_runtime, live_layers)
            .await
            .expect("failed to create runtime in storage");

        // Test - insert data
        let key = "some_field".to_string();
        let value = "some value".to_string();
        match runtime.add_annotation(&key, &value, limit).await {
            Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
                panic!("Writing annotations should fail when using EncodingFormat::Legacy")
            }
            Ok(_) => {}
            Err(_) if write_encoding_format == EncodingFormat::Legacy => {
                // This error is expected
                return;
            }
            Err(e) => {
                panic!("Error adding annotations: {e}")
            }
        };

        // Test - insert some more data
        let value2 = "some other value".to_string();
        assert!(runtime.add_annotation(&key, &value2, limit).await.is_ok());

        // Test - retrieve data - the first inserted data should be the
        // what is retrieved because of how adding to the runtime stack
        // works.
        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!();
        };

        let result = runtime.annotation(&key).await.unwrap();
        assert!(result.is_some());

        assert!(value == *result.unwrap());
    }
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_storage_runtime_add_annotations_list(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let root = tmpdir.path().to_string_lossy().to_string();
        let repo = crate::storage::RepositoryHandle::from(
            crate::storage::fs::MaybeOpenFsRepository::create(root)
                .await
                .unwrap(),
        );
        let storage = Storage::new(repo).unwrap();
        let limit: usize = 16 * 1024;

        let keep_runtime = false;
        let live_layers = Vec::new();
        let mut runtime = storage
            .create_named_runtime("test-with-annotation-data", keep_runtime, live_layers)
            .await
            .expect("failed to create runtime in storage");

        // Test - insert data
        let key = "some_field".to_string();
        let value = "some value".to_string();
        let key2 = "some_other_field".to_string();
        let value2 = "some other value".to_string();

        let annotations: Vec<KeyValuePair> = vec![(&key, &value), (&key2, &value2)];

        match runtime.add_annotations(annotations, limit).await {
            Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
                panic!("Writing annotations should fail when using EncodingFormat::Legacy")
            }
            Ok(_) => {}
            Err(_) if write_encoding_format == EncodingFormat::Legacy => {
                // This error is expected
                return;
            }
            Err(e) => {
                panic!("Error adding annotations: {e}")
            }
        };

        // Test - retrieve data both pieces of data
        let result = runtime.annotation(&key).await.unwrap();
        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!()
        } else {
            assert!(result.is_some());
            assert!(value == *result.unwrap());
        }

        let result2 = runtime.annotation(&key2).await.unwrap();
        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!()
        } else {
            assert!(result2.is_some());
            assert!(value2 == *result2.unwrap());
        }
    }
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_storage_runtime_with_nested_annotation(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        // Setup the objects needed for the runtime used in the test
        let root = tmpdir.path().to_string_lossy().to_string();
        let repo = crate::storage::RepositoryHandle::from(
            crate::storage::fs::MaybeOpenFsRepository::create(root)
                .await
                .unwrap(),
        );

        // make an annotation layer
        let key = "some_field";
        let value = "somevalue";
        let annotation_value = AnnotationValue::string(value);
        let layer = Layer::new_with_annotation(key, annotation_value);
        match repo.write_object(&layer).await {
            Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
                panic!("Writing annotations should fail when using EncodingFormat::Legacy")
            }
            Ok(_) => {}
            Err(_) if write_encoding_format == EncodingFormat::Legacy => {
                // This error is expected
                return;
            }
            Err(e) => {
                panic!("Error adding annotations: {e}")
            }
        };

        // make a platform that contains the annotation layer
        let layers: Vec<encoding::Digest> = vec![layer.digest().unwrap()];
        let platform = Platform::from_iter(layers);
        repo.write_object(&platform.clone()).await.unwrap();

        // put the platform into a runtime
        let storage = Storage::new(repo).unwrap();
        let keep_runtime = false;
        let live_layers = Vec::new();
        let mut runtime = storage
            .create_named_runtime("test-with-annotation-nested", keep_runtime, live_layers)
            .await
            .expect("failed to create runtime in storage");
        runtime.push_digest(platform.digest().unwrap());

        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!();
        };

        // Test - retrieve the data even though it is nested inside a
        // platform object in the runtime.
        let result = runtime.annotation(key).await.unwrap();
        assert!(result.is_some());

        assert!(value == result.unwrap());
    }
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_storage_runtime_with_annotation_all(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        let root = tmpdir.path().to_string_lossy().to_string();
        let repo = crate::storage::RepositoryHandle::from(
            crate::storage::fs::MaybeOpenFsRepository::create(root)
                .await
                .unwrap(),
        );
        let storage = Storage::new(repo).unwrap();
        let limit: usize = 16 * 1024;

        let keep_runtime = false;
        let live_layers = Vec::new();
        let mut runtime = storage
            .create_named_runtime("test-with-annotation-all", keep_runtime, live_layers)
            .await
            .expect("failed to create runtime in storage");

        // Test - insert two distinct data values
        let key = "some_field";
        let value = "somevalue";

        match runtime.add_annotation(key, value, limit).await {
            Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
                panic!("Writing annotations should fail when using EncodingFormat::Legacy")
            }
            Ok(_) => {}
            Err(_) if write_encoding_format == EncodingFormat::Legacy => {
                // This error is expected
                return;
            }
            Err(e) => {
                panic!("Error adding annotations: {e}")
            }
        };

        let key2 = "some_field2";
        let value2 = "somevalue2";
        assert!(runtime.add_annotation(key2, value2, limit).await.is_ok());

        // Test - get all the data back out
        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!();
        };

        let result = runtime.all_annotations().await.unwrap();

        assert!(result.len() == 2);
        for (expected_key, expected_value) in [(key, value), (key2, value2)].iter() {
            assert!(result.contains_key(*expected_key));
            match result.get(*expected_key) {
                Some(v) => {
                    assert!(v == expected_value);
                }
                None => panic!("Value missing for {expected_key} when getting all annotation"),
            }
        }
    }
}

#[rstest(
    write_encoding_format => [EncodingFormat::Legacy, EncodingFormat::FlatBuffers],
    write_digest_strategy => [DigestStrategy::Legacy, DigestStrategy::WithKindAndSalt],
)]
#[tokio::test]
#[serial_test::serial(config)]
async fn test_storage_runtime_with_nested_annotation_all(
    tmpdir: tempfile::TempDir,
    write_encoding_format: EncodingFormat,
    write_digest_strategy: DigestStrategy,
) {
    reset_config_async! {
        let mut config = Config::default();
        config.storage.encoding_format = write_encoding_format;
        config.storage.digest_strategy = write_digest_strategy;
        config.make_current().unwrap();

        // setup the objects needed for the runtime used in the test
        let root = tmpdir.path().to_string_lossy().to_string();
        let repo = crate::storage::RepositoryHandle::from(
            crate::storage::fs::MaybeOpenFsRepository::create(root)
                .await
                .unwrap(),
        );

        // make two distinct data values
        let key = "some_field";
        let value = "somevalue";
        let annotation_value = AnnotationValue::string(value);
        let layer = Layer::new_with_annotation(key, annotation_value);
        match repo.write_object(&layer.clone()).await {
            Ok(_) if write_encoding_format == EncodingFormat::Legacy => {
                panic!("Writing annotations should fail when using EncodingFormat::Legacy")
            }
            Ok(_) => {}
            Err(_) if write_encoding_format == EncodingFormat::Legacy => {
                // This error is expected
                return;
            }
            Err(e) => {
                panic!("Error adding annotations: {e}")
            }
        };

        let key2 = "some_field2";
        let value2 = "somevalue2";
        let annotation_value2 = AnnotationValue::string(value2);
        let layer2 = Layer::new_with_annotation(key2, annotation_value2);
        repo.write_object(&layer2.clone()).await.unwrap();

        // make a platform with one annotation layer
        let layers: Vec<encoding::Digest> = vec![layer.digest().unwrap()];
        let platform = Platform::from_iter(layers);
        repo.write_object(&platform.clone()).await.unwrap();

        // make another platform with the first platform and the other
        // annotation layer. this second platform is in the runtime
        let layers2: Vec<encoding::Digest> = vec![platform.digest().unwrap(), layer2.digest().unwrap()];
        let platform2 = Platform::from_iter(layers2);
        repo.write_object(&platform2.clone()).await.unwrap();

        // finally set up the runtime
        let storage = Storage::new(repo).unwrap();

        let keep_runtime = false;
        let live_layers = Vec::new();
        let mut runtime = storage
            .create_named_runtime("test-with-annotation-all-nested", keep_runtime, live_layers)
            .await
            .expect("failed to create runtime in storage");
        runtime.push_digest(platform2.digest().unwrap());

        // Test - get all the data back out even thought it is nested at
        // different levels in different platform objects in the runtime
        if write_encoding_format == EncodingFormat::Legacy {
            unreachable!();
        };

        let result = runtime.all_annotations().await.unwrap();
        assert!(result.len() == 2);
        for (expected_key, expected_value) in [(key, value), (key2, value2)].iter() {
            assert!(result.contains_key(*expected_key));
            match result.get(*expected_key) {
                Some(v) => {
                    assert!(v == expected_value);
                }
                None => panic!("Value missing for {expected_key} when getting all annotations"),
            }
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_storage_remove_runtime(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::MaybeOpenFsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo).unwrap();

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
        crate::storage::fs::MaybeOpenFsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo).unwrap();

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
        crate::storage::fs::MaybeOpenFsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo).unwrap();

    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("failed to create runtime in storage");
    let upper_dir = tmpdir.path().join("upper");
    runtime.data.config.upper_dir.clone_from(&upper_dir);

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
        crate::storage::fs::MaybeOpenFsRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = Storage::new(repo).unwrap();

    let dir = "/tmp";
    let mountpoint = "tests/tests/tests".to_string();
    let mount = BindMount {
        src: dir.into(),
        dest: mountpoint,
    };
    let live_layer = LiveLayer {
        api: SpecApiVersion::V0Layer,
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

#[cfg(unix)]
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
