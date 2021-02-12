macro_rules! fixtures {
    () => {
        use rstest::fixture;
        use tempdir::TempDir;

        use crate as spfs;
        #[fixture]
        fn tmpdir() -> TempDir {
            TempDir::new("spfs-test-").expect("failed to create dir for test")
        }

        #[fixture(kind = "fs")]
        fn tmprepo(kind: &str) -> spfs::storage::RepositoryHandle {
            let tmpdir = tmpdir();
            match kind {
                "fs" => spfs::storage::fs::FSRepository::create(tmpdir.path().join("repo"))
                    .unwrap()
                    .into(),
                "tar" => spfs::storage::tar::TarRepository::create(tmpdir.path().join("repo.tar"))
                    .unwrap()
                    .into(),
                _ => panic!("unknown repo kind '{}'", kind),
            }
        }
    };
}
