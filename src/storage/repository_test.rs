// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Repository;
use crate::{api, fixtures::*, Error};

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_list_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // assert repo.list_packages() == [], "should not fail when empty"
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_list_package_versions_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // assert (
    //     list(repo.list_package_versions("nothing")) == []
    // ), "should not fail with unknown package"
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_list_package_builds_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // nothing = api.parse_ident("nothing/1.0.0")
    // assert (
    //     list(repo.list_package_builds(nothing)) == []
    // ), "should not fail with unknown package"
    todo!();
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_read_spec_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // with pytest.raises(PackageNotFoundError):
    //     repo.read_spec(api.parse_ident("nothing"))
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_get_package_empty(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // with pytest.raises(PackageNotFoundError):
    //     repo.get_package(api.parse_ident("nothing/1.0.0/src"))
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_publish_spec(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    // repo.publish_spec(spec)
    // assert list(repo.list_packages()) == ["my-pkg"]
    // assert list(repo.list_package_versions("my-pkg")) == ["1.0.0"]

    // with pytest.raises(VersionExistsError):
    //     repo.publish_spec(spec)
    // repo.force_publish_spec(spec)
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_publish_package(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    // repo.publish_spec(spec)
    // spec.pkg = spec.pkg.with_build("7CI5R7Y4")
    // repo.publish_package(spec, {"run": spkrs.EMPTY_DIGEST})
    // assert list(repo.list_package_builds(spec.pkg)) == [spec.pkg]
    // assert repo.read_spec(spec.pkg) == spec
    todo!()
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::spfs(RepoKind::SPFS)]
fn test_repo_remove_package(#[case] repo: RepoKind) {
    let _guard = crate::HANDLE.enter();
    let repo = crate::HANDLE.block_on(make_repo(repo));
    // spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    // repo.publish_spec(spec)
    // spec.pkg = spec.pkg.with_build("7CI5R7Y4")
    // repo.publish_package(spec, {"run": spkrs.EMPTY_DIGEST})
    // assert list(repo.list_package_builds(spec.pkg)) == [spec.pkg]
    // assert repo.read_spec(spec.pkg) == spec
    // repo.remove_package(spec.pkg)
    // assert list(repo.list_package_builds(spec.pkg)) == []
    todo!()
}
