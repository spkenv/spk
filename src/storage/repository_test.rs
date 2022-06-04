// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::{api, fixtures::*, Error};

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_list_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    assert!(
        repo.list_packages().unwrap().is_empty(),
        "should not fail when empty"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_list_package_versions_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    assert!(
        repo.list_package_versions(&"nothing".parse().unwrap())
            .unwrap()
            .is_empty(),
        "should not fail with unknown package"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_list_package_builds_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let nothing = api::parse_ident("nothing/1.0.0").unwrap();
    assert!(
        repo.list_package_builds(&nothing).unwrap().is_empty(),
        "should not fail with unknown package"
    );
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_read_spec_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let nothing = api::parse_ident("nothing").unwrap();
    match repo.read_spec(&nothing) {
        Err(Error::PackageNotFoundError(_)) => (),
        _ => panic!("expected package not found error"),
    }
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_get_package_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let nothing = api::parse_ident("nothing/1.0.0/src").unwrap();
    match repo.read_spec(&nothing) {
        Err(Error::PackageNotFoundError(_)) => (),
        _ => panic!("expected package not found error"),
    }
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_publish_spec(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(spec.clone()).unwrap();
    assert_eq!(repo.list_packages().unwrap(), vec![spec.pkg.name.clone()]);
    assert_eq!(
        repo.list_package_versions(&spec.pkg.name).unwrap(),
        vec!["1.0.0"]
    );

    match repo.publish_spec(spec.clone()) {
        Err(Error::VersionExistsError(_)) => (),
        _ => panic!("expected version exists error"),
    }
    repo.force_publish_spec(spec)
        .expect("force publish should ignore existing version");
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_publish_package(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let mut spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(spec.clone()).unwrap();
    spec.pkg
        .set_build(Some(api::parse_build("7CI5R7Y4").unwrap()));
    repo.publish_package(
        spec.clone(),
        vec![(api::Component::Run, empty_layer_digest())]
            .into_iter()
            .collect(),
    )
    .unwrap();
    assert_eq!(
        repo.list_package_builds(&spec.pkg).unwrap(),
        [spec.pkg.clone()]
    );
    assert_eq!(repo.read_spec(&spec.pkg).unwrap(), spec);
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::Spfs)]
fn test_repo_remove_package(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    let mut spec = crate::spec!({"pkg": "my-pkg/1.0.0"});
    repo.publish_spec(spec.clone()).unwrap();
    spec.pkg
        .set_build(Some(api::parse_build("7CI5R7Y4").unwrap()));
    repo.publish_package(
        spec.clone(),
        vec![(api::Component::Run, empty_layer_digest())]
            .into_iter()
            .collect(),
    )
    .unwrap();
    assert_eq!(
        repo.list_package_builds(&spec.pkg).unwrap(),
        vec![spec.pkg.clone()]
    );
    assert_eq!(repo.read_spec(&spec.pkg).unwrap(), spec);
    repo.remove_package(&spec.pkg).unwrap();
    assert!(repo.list_package_builds(&spec.pkg).unwrap().is_empty());
}
