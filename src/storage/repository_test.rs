// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::{api, fixtures::*, storage::CachePolicy, Error};

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_list_empty(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    assert!(
        repo.list_packages().await.unwrap().is_empty(),
        "should not fail when empty"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_list_package_versions_empty(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    assert!(
        repo.list_package_versions(api::PkgName::new("nothing").unwrap())
            .await
            .unwrap()
            .is_empty(),
        "should not fail with unknown package"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_list_package_builds_empty(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let nothing = api::parse_ident("nothing/1.0.0").unwrap();
    assert!(
        repo.list_package_builds(&nothing).await.unwrap().is_empty(),
        "should not fail with unknown package"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_read_spec_empty(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let nothing = api::parse_ident("nothing").unwrap();
    match repo.read_spec(&nothing).await {
        Err(Error::PackageNotFoundError(_)) => (),
        _ => panic!("expected package not found error"),
    }
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_get_package_empty(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let nothing = api::parse_ident("nothing/1.0.0/src").unwrap();
    match repo.read_spec(&nothing).await {
        Err(Error::PackageNotFoundError(_)) => (),
        _ => panic!("expected package not found error"),
    }
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_publish_spec(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(&spec).await.unwrap();
    assert_eq!(
        repo.list_packages().await.unwrap(),
        vec![spec.pkg.name.clone()]
    );
    assert_eq!(
        repo.list_package_versions(&spec.pkg.name)
            .await
            .unwrap()
            .iter()
            .map(|v| (**v).clone())
            .collect::<Vec<_>>(),
        vec!["1.0.0"]
    );

    match repo.publish_spec(&spec).await {
        Err(Error::VersionExistsError(_)) => (),
        _ => panic!("expected version exists error"),
    }
    repo.force_publish_spec(&spec)
        .await
        .expect("force publish should ignore existing version");
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_publish_package(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let mut spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(&spec).await.unwrap();
    spec.pkg
        .set_build(Some(api::parse_build("7CI5R7Y4").unwrap()));
    repo.publish_package(
        &spec,
        vec![(api::Component::Run, empty_layer_digest())]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap();
    assert_eq!(
        repo.list_package_builds(&spec.pkg).await.unwrap(),
        [spec.pkg.clone()]
    );
    assert_eq!(*repo.read_spec(&spec.pkg).await.unwrap(), spec);
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
#[tokio::test]
async fn test_repo_remove_package(#[case] repo: RepoKind) {
    let repo = make_repo(repo).await;
    let mut spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(&spec).await.unwrap();
    spec.pkg
        .set_build(Some(api::parse_build("7CI5R7Y4").unwrap()));
    repo.publish_package(
        &spec,
        vec![(api::Component::Run, empty_layer_digest())]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap();
    assert_eq!(
        repo.list_package_builds(&spec.pkg).await.unwrap(),
        vec![spec.pkg.clone()]
    );
    assert_eq!(*repo.read_spec(&spec.pkg).await.unwrap(), spec);
    repo.remove_package(&spec.pkg).await.unwrap();
    assert!(crate::with_cache_policy!(repo, CachePolicy::BypassCache, {
        repo.list_package_builds(&spec.pkg)
    })
    .await
    .unwrap()
    .is_empty());
}
