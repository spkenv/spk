macro_rules! fixtures {
    () => {
        use rstest::fixture;
        use tempdir::TempDir;

        type TempRepo = (TempDir, spfs::storage::RepositoryHandle);

        #[allow(dead_code)]
        fn init_logging() {
            let sub = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_test_writer()
                .finish();
            tracing::subscriber::set_global_default(sub)
                .or::<Result<(), ()>>(Ok(()))
                .unwrap();
        }

        use crate as spfs;
        #[fixture]
        fn tmpdir() -> TempDir {
            TempDir::new("spfs-test-").expect("failed to create dir for test")
        }

        #[fixture(kind = "fs")]
        fn tmprepo(kind: &str) -> (tempdir::TempDir, spfs::storage::RepositoryHandle) {
            let tmpdir = tmpdir();
            let repo = match kind {
                "fs" => spfs::storage::fs::FSRepository::create(tmpdir.path().join("repo"))
                    .unwrap()
                    .into(),
                "tar" => spfs::storage::tar::TarRepository::create(tmpdir.path().join("repo.tar"))
                    .unwrap()
                    .into(),
                _ => panic!("unknown repo kind '{}'", kind),
            };
            (tmpdir, repo)
        }
    };
}
