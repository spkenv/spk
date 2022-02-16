use std::str::FromStr;

use rstest::rstest;
use spfs::prelude::*;

use super::SPFSRepository;
use crate::storage::Repository;

use crate::fixtures::*;

#[rstest]
fn test_repo_meta_tag_is_valid() {
    spfs::tracking::TagSpec::parse(super::REPO_METADATA_TAG)
        .expect("repo metadata tag must be a valid spfs tag");
}

#[rstest]
fn test_repo_version_is_valid() {
    crate::api::Version::from_str(super::REPO_VERSION)
        .expect("repo current version must be a valid spk version string");
}

#[rstest]
fn test_metadata_io(tmpdir: tempdir::TempDir) {
    init_logging();
    let repo_root = tmpdir.path();
    let mut repo =
        SPFSRepository::from(spfs::storage::fs::FSRepository::create(repo_root).unwrap());

    let meta = super::RepositoryMetadata::default();
    repo.write_metadata(&meta).unwrap();
    let actual = repo.read_metadata().unwrap();
    assert_eq!(actual, meta, "should return metadata as it was stored");
}

#[rstest]
fn test_upgrade_sets_version(tmpdir: tempdir::TempDir) {
    init_logging();
    let current_version = crate::api::Version::from_str(super::REPO_VERSION).unwrap();
    let repo_root = tmpdir.path();
    let mut repo =
        SPFSRepository::from(spfs::storage::fs::FSRepository::create(repo_root).unwrap());

    assert_eq!(repo.read_metadata().unwrap().version, Default::default());
    repo.upgrade()
        .expect("upgrading an empty repo should succeed");
    assert_eq!(repo.read_metadata().unwrap().version, current_version);
}

#[rstest]
fn test_upgrade_changes_tags(tmpdir: tempdir::TempDir) {
    init_logging();
    let repo_root = tmpdir.path();
    let mut spfs_repo = spfs::storage::fs::FSRepository::create(repo_root).unwrap();
    let mut repo = SPFSRepository::new(&format!("file://{}", repo_root.display())).unwrap();

    let ident = crate::api::Ident::from_str("mypkg/1.0.0/src").unwrap();

    // publish an "old style" package spec and build
    let mut old_path =
        spfs::tracking::TagSpec::from_str(repo.build_package_tag(&ident).unwrap().as_str())
            .unwrap();
    spfs_repo
        .push_tag(&old_path, &spfs::encoding::EMPTY_DIGEST.into())
        .unwrap();
    old_path = spfs::tracking::TagSpec::from_str(repo.build_spec_tag(&ident).as_str()).unwrap();
    spfs_repo
        .push_tag(&old_path, &spfs::encoding::EMPTY_DIGEST.into())
        .unwrap();

    let pkg = repo.lookup_package(&ident).unwrap();
    assert!(matches!(pkg, super::StoredPackage::WithoutComponents(_)));

    repo.upgrade()
        .expect("upgrading a simple repo should succeed");

    let pkg = repo.lookup_package(&ident).unwrap();
    assert!(matches!(pkg, super::StoredPackage::WithComponents(_)));
}
