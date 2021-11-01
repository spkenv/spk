// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate as spfs;
use rstest::fixture;
use tempdir::TempDir;

pub enum TempRepo {
    FS(spfs::storage::RepositoryHandle, TempDir),
    Tar(spfs::storage::RepositoryHandle, TempDir),
    Rpc(
        spfs::storage::RepositoryHandle,
        Option<std::thread::JoinHandle<()>>,
        std::sync::mpsc::Sender<()>,
        TempDir,
    ),
}

impl std::ops::Deref for TempRepo {
    type Target = spfs::storage::RepositoryHandle;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::FS(r, _) => r,
            Self::Tar(r, _) => r,
            Self::Rpc(r, ..) => r,
        }
    }
}

impl std::ops::DerefMut for TempRepo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::FS(r, _) => r,
            Self::Tar(r, _) => r,
            Self::Rpc(r, ..) => r,
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        if let Self::Rpc(_, join_handle, shutdown, _) = self {
            shutdown
                .send(())
                .expect("failed to send server shutdown signal");
            join_handle
                .take()
                .map(|h| h.join().expect("failed to join server thread"));
        }
    }
}

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

#[fixture]
pub fn spfs_binary() -> std::path::PathBuf {
    static BUILD_BIN: std::sync::Once = std::sync::Once::new();
    BUILD_BIN.call_once(|| {
        let mut command = std::process::Command::new(std::env::var("CARGO").unwrap());
        command.args(&["build", "--all"]);
        let code = command
            .status()
            .expect("failed to build binary to test with")
            .code();
        if Some(0) != code {
            panic!("failed to build binary to test with: {:?}", code);
        };
    });
    let mut path = std::env::current_exe().expect("test must have current binary path");
    loop {
        let parent = path.parent();
        if parent.is_none() {
            panic!("cannot find spfs binary to test");
        }
        let parent = parent.unwrap();
        if parent.is_dir() && parent.file_name() == Some(std::ffi::OsStr::new("debug")) {
            path.pop();
            break;
        }

        path.pop();
    }
    path.push(env!("CARGO_PKG_NAME").to_string());
    path
}

#[fixture]
pub fn tmpdir() -> TempDir {
    TempDir::new("spfs-test-").expect("failed to create dir for test")
}

#[fixture(kind = "fs")]
pub async fn tmprepo(kind: &str) -> TempRepo {
    init_logging();
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
            let repo = spfs::storage::tar::TarRepository::create(tmpdir.path().join("repo.tar"))
                .await
                .unwrap()
                .into();
            TempRepo::Tar(repo, tmpdir)
        }
        #[cfg(feature = "server")]
        "rpc" => {
            let repo = spfs::storage::fs::FSRepository::create(tmpdir.path().join("repo"))
                .await
                .unwrap()
                .into();
            let listen: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
            let (shutdown_send, shutdown_recv) = std::sync::mpsc::channel::<()>();
            let (addr_send, addr_recv) = std::sync::mpsc::channel::<std::net::SocketAddr>();
            let server_join_handle = tokio::task::spawn(async move {
                // this separate context needs it's own logger in order to
                // output properly (since we are spawning before the test even starts)
                let _guard = init_logging();
                let listener = tokio::net::TcpListener::bind(listen).await.unwrap();
                let local_addr = listener.local_addr().unwrap();
                let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
                let future = tonic::transport::Server::builder()
                    .add_service(spfs::server::Service::new_srv(repo))
                    .serve_with_incoming_shutdown(incoming, async move {
                        // use a blocking task to avoid locking up the whole server
                        // with this very synchronus channel recv process
                        tokio::task::spawn_blocking(move || {
                            shutdown_recv
                                .recv()
                                .expect("failed to get server shutdown signal");
                        })
                        .await
                        .unwrap()
                    });
                tracing::debug!("test server listening: {}", local_addr);
                addr_send
                    .send(local_addr)
                    .expect("failed to report server address");
                future.await.expect("test server failed");
            });
            let addr = addr_recv.recv().expect("failed to recieve server address");
            let url = format!("http2://{}", addr).parse().unwrap();
            tracing::debug!("Connected to rpc test repo: {}", url);
            let repo = spfs::storage::rpc::RpcRepository::connect(url)
                .unwrap()
                .into();
            TempRepo::Rpc(repo, Some(server_join_handle), shutdown_send, tmpdir)
        }
        _ => panic!("unknown repo kind '{}'", kind),
    }
}

pub fn ensure(path: std::path::PathBuf, data: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("failed to make dirs");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .expect("failed to create file");
    std::io::copy(&mut data.as_bytes(), &mut file).expect("failed to write file data");
}
