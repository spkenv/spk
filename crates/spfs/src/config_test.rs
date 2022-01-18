// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Config;

#[rstest]
fn test_config_list_remote_names_empty() {
    let config = Config::default();
    assert_eq!(config.list_remote_names().len(), 0)
}

#[rstest]
fn test_config_list_remote_names() {
    let config = Config::load_string("[remote.origin]\naddress=http://myaddres").unwrap();
    assert_eq!(config.list_remote_names(), vec!["origin".to_string()]);
}

#[rstest]
#[tokio::test]
async fn test_config_get_remote_unknown() {
    let config = Config::default();
    config
        .get_remote("unknown")
        .await
        .expect_err("should fail to load unknown config");
}

#[rstest]
#[tokio::test]
async fn test_config_get_remote() {
    let tmpdir = tempdir::TempDir::new("spfs-test").unwrap();
    let remote = tmpdir.path().join("remote");
    let _ = crate::storage::fs::FSRepository::create(&remote).await.unwrap();

    let config = Config::load_string(format!(
        "[remote.origin]\naddress=file://{}",
        &remote.to_string_lossy()
    ))
    .unwrap();
    let repo = config.get_remote("origin").await;
    assert!(repo.is_ok());
}
