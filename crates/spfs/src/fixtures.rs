// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

macro_rules! fixtures {
    () => {
        use rstest::fixture;
        use tempdir::TempDir;

        #[allow(dead_code)]
        enum TempRepo {
            FS(spfs::storage::RepositoryHandle, TempDir),
            Tar(spfs::storage::RepositoryHandle, TempDir),
            Rpc(spfs::storage::RepositoryHandle, std::process::Child),
        }

        impl std::ops::Deref for TempRepo {
            type Target = spfs::storage::RepositoryHandle;
            fn deref(&self) -> &Self::Target {
                match self {
                    Self::FS(r, _) => r,
                    Self::Tar(r, _) => r,
                    Self::Rpc(r, _) => r,
                }
            }
        }

        impl std::ops::DerefMut for TempRepo {
            fn deref_mut(&mut self) -> &mut Self::Target {
                match self {
                    Self::FS(r, _) => r,
                    Self::Tar(r, _) => r,
                    Self::Rpc(r, _) => r,
                }
            }
        }

        impl Drop for TempRepo {
            fn drop(&mut self) {
                match self {
                    Self::Rpc(_, child) => {
                        let _ = child.kill();
                    }
                    _ => (),
                }
            }
        }

        #[allow(dead_code)]
        fn init_logging() {
            let sub = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_test_writer()
                .finish();
            let _ = tracing::subscriber::set_global_default(sub);
        }

        #[fixture]
        fn spfs_binary() -> std::path::PathBuf {
            static BUILD_BIN: std::sync::Once = std::sync::Once::new();
            BUILD_BIN.call_once(|| {
                let mut command = std::process::Command::new(std::env::var("CARGO").unwrap());
                command.args(&["build", "--all"]);
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
        async fn tmprepo(kind: &str) -> TempRepo {
            let tmpdir = tmpdir();
            match kind {
                "fs" => {
                    let repo = spfs::storage::fs::FSRepository::create(tmpdir.path().join("repo"))
                        .await
                        .unwrap()
                        .into();
                    TempRepo::FS(repo, tmpdir)
                }
                "tar" => {
                    let repo =
                        spfs::storage::tar::TarRepository::create(tmpdir.path().join("repo.tar"))
                            .await
                            .unwrap()
                            .into();
                    TempRepo::Tar(repo, tmpdir)
                }
                "rpc" => {
                    let server_binary = spfs_binary().with_file_name("spfs-server");
                    let child = std::process::Command::new(server_binary)
                        .arg("http://localhost:7737")
                        .arg("-vvv")
                        .spawn()
                        .expect("failed to start server for test");
                    let repo = spfs::storage::rpc::RpcRepository::connect(
                        "http2://localhost:7737".parse().unwrap(),
                    )
                    .await
                    .unwrap()
                    .into();
                    TempRepo::Rpc(repo, child)
                }
                _ => panic!("unknown repo kind '{}'", kind),
            }
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
