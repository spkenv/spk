use rstest::rstest;

use crate::storage::Repository;

use super::SPFSRepository;

crate::fixtures!();

#[rstest]
fn test_repo_meta_tag_is_valid() {
    spfs::tracking::TagSpec::parse(super::REPO_METADATA_TAG)
        .expect("repo metadata tag must be a valid spfs tag");
}

#[rstest]
fn test_metadata_io() {
    init_logging();
    let dir = tempdir::TempDir::new("spk_test").unwrap();
    let repo_root = dir.path();
    let mut repo =
        SPFSRepository::from(spfs::storage::fs::FSRepository::create(repo_root).unwrap());

    let meta = super::RepositoryMetadata::default();
    repo.write_metadata(&meta).unwrap();
    let actual = repo.read_metadata().unwrap();
    assert_eq!(actual, meta, "should return metadata as it was stored");
}
