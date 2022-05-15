// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::config::RemoteSpecifier;

use super::Config;

#[rstest]
fn test_config_list_remote_names_empty() {
    let config = Config::default();
    assert_eq!(config.list_remote_names().len(), 0)
}

#[rstest]
fn test_config_list_remote_names() {
    let config: Config =
        serde_json::from_str(r#"{"remote": { "origin": { "address": "http://myaddress" } } }"#)
            .unwrap();
    assert_eq!(config.list_remote_names(), vec!["origin".to_string()]);
}

#[rstest]
#[tokio::test]
async fn test_config_get_remote_unknown() {
    let config = Config::default();
    config
        .get_remote(RemoteSpecifier::Name("unknown"))
        .await
        .expect_err("should fail to load unknown config");
}

#[rstest]
#[tokio::test]
async fn test_config_get_remote() {
    let tmpdir = tempdir::TempDir::new("spfs-test").unwrap();
    let remote = tmpdir.path().join("remote");
    let _ = crate::storage::fs::FSRepository::create(&remote)
        .await
        .unwrap();

    let config: Config = serde_json::from_str(&format!(
        r#"{{"remote": {{ "origin": {{ "address": "file://{}" }} }} }}"#,
        &remote.to_string_lossy()
    ))
    .unwrap();
    let repo = config.get_remote(RemoteSpecifier::Name("origin")).await;
    assert!(repo.is_ok());
}

#[rstest]
#[case(
    r#"
{
    "remote": {
        "addressed": {
            "address": "file:/some/path"
        },
        "configured": {
            "scheme": "fs",
            "path": "/some/path"
        }
    }
}"#
)]
fn test_remote_config_or_address(#[case] source: &str) {
    let _config: Config = serde_json::from_str(source).expect("config should have loaded properly");
}
