macro_rules! fixtures {
    () => {
        use rstest::fixture;
        use tempdir::TempDir;

        #[allow(dead_code)]
        type TempRepo = (TempDir, spfs::storage::RepositoryHandle);

        #[allow(dead_code)]
        fn init_logging() -> tracing::dispatcher::DefaultGuard {
            let sub = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_test_writer()
                .finish();
            tracing::subscriber::set_default(sub)
        }

        #[fixture]
        fn spfs_binary() -> std::path::PathBuf {
            static BUILD_BIN: std::sync::Once = std::sync::Once::new();
            BUILD_BIN.call_once(|| {
                let mut command = std::process::Command::new(std::env::var("CARGO").unwrap());
                command.arg("build");
                if Some(0)
                    != command
                        .status()
                        .expect("failed to build binary to test with")
                        .code()
                {
                    panic!("failed to build binary to test with");
                };
            });
            let mut path = std::env::current_exe().expect("test must have current binary path");
            loop {
                {
                    let parent = path.parent();
                    if parent.is_none() {
                        panic!("cannot find spfs binary to test");
                    }
                    let parent = parent.unwrap();
                    if parent.is_dir() && parent.file_name().unwrap() == "target" {
                        break;
                    }
                }
                path.pop();
            }

            path.push(env!("CARGO_PKG_NAME").to_string());
            path
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

        #[allow(dead_code)]
        fn ensure(path: std::path::PathBuf, data: &str) {
            std::fs::create_dir_all(path.parent().unwrap()).expect("failed to make dirs");
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(path)
                .expect("failed to create file");
            std::io::copy(&mut data.as_bytes(), &mut file).expect("failed to write file data");
        }
    };
}
