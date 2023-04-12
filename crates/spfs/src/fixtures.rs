// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rand::distributions::{Alphanumeric, DistString};
use rand::Rng;
use rstest::fixture;
use tempfile::TempDir;

use crate as spfs;

pub enum TempRepo {
    FS(Arc<spfs::storage::RepositoryHandle>, TempDir),
    Tar(Arc<spfs::storage::RepositoryHandle>, TempDir),
    Rpc {
        repo: Arc<spfs::storage::RepositoryHandle>,
        grpc_join_handle: Option<tokio::task::JoinHandle<()>>,
        http_join_handle: Option<tokio::task::JoinHandle<()>>,
        grpc_shutdown: std::sync::mpsc::Sender<()>,
        http_shutdown: std::sync::mpsc::Sender<()>,
        tmpdir: TempDir,
    },
}

impl TempRepo {
    pub fn repo(&self) -> Arc<spfs::storage::RepositoryHandle> {
        match self {
            Self::FS(r, _) => Arc::clone(r),
            Self::Tar(r, _) => Arc::clone(r),
            Self::Rpc { repo, .. } => Arc::clone(repo),
        }
    }
}

impl std::ops::Deref for TempRepo {
    type Target = spfs::storage::RepositoryHandle;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::FS(r, _) => r,
            Self::Tar(r, _) => r,
            Self::Rpc { repo, .. } => repo,
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        if let Self::Rpc {
            grpc_shutdown,
            http_shutdown,
            ..
        } = self
        {
            grpc_shutdown
                .send(())
                .expect("failed to send grpc server shutdown signal");
            http_shutdown
                .send(())
                .expect("failed to send http server shutdown signal");
        }
    }
}

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter("spfs=trace")
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
        command.args(["build", "--all"]);
        let code = command
            .status()
            .expect("failed to build binary to test with")
            .code();
        if Some(0) != code {
            panic!("failed to build binary to test with: {code:?}");
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
    path.push(env!("CARGO_PKG_NAME"));
    path
}

#[fixture]
pub fn tmpdir() -> TempDir {
    tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .expect("failed to create dir for test")
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
            TempRepo::FS(Arc::new(repo), tmpdir)
        }
        "tar" => {
            let repo = spfs::storage::tar::TarRepository::create(tmpdir.path().join("repo.tar"))
                .await
                .unwrap()
                .into();
            TempRepo::Tar(Arc::new(repo), tmpdir)
        }
        #[cfg(feature = "server")]
        "rpc" => {
            use crate::storage::prelude::*;
            let repo = std::sync::Arc::new(spfs::storage::RepositoryHandle::FS(
                spfs::storage::fs::FSRepository::create(tmpdir.path().join("repo"))
                    .await
                    .unwrap(),
            ));
            let listen: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
            let http_listener = std::net::TcpListener::bind(listen).unwrap();
            let local_http_addr = http_listener.local_addr().unwrap();
            let payload_service = spfs::server::PayloadService::new(
                repo.clone(),
                format!("http://{local_http_addr}").parse().unwrap(),
            );
            let (grpc_shutdown, grpc_shutdown_recv) = std::sync::mpsc::channel::<()>();
            let (http_shutdown, http_shutdown_recv) = std::sync::mpsc::channel::<()>();
            let grpc_listener = tokio::net::TcpListener::bind(listen).await.unwrap();
            let local_grpc_addr = grpc_listener.local_addr().unwrap();
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(grpc_listener);
            let grpc_future = tonic::transport::Server::builder()
                .add_service(spfs::server::Repository::new_srv())
                .add_service(spfs::server::TagService::new_srv(repo.clone()))
                .add_service(spfs::server::DatabaseService::new_srv(repo))
                .add_service(payload_service.clone().into_srv())
                .serve_with_incoming_shutdown(incoming, async move {
                    // use a blocking task to avoid locking up the whole server
                    // with this very synchronous channel recv process
                    tokio::task::spawn_blocking(move || {
                        grpc_shutdown_recv
                            .recv()
                            .expect("failed to get server shutdown signal");
                    })
                    .await
                    .unwrap()
                });
            tracing::debug!("test rpc server listening: {local_grpc_addr}");
            let grpc_join_handle =
                tokio::task::spawn(async move { grpc_future.await.expect("test server failed") });
            let http_server = {
                hyper::Server::from_tcp(http_listener).unwrap().serve(
                    hyper::service::make_service_fn(move |_| {
                        let s = payload_service.clone();
                        async move { Ok::<_, std::convert::Infallible>(s) }
                    }),
                )
            };
            let http_future = http_server.with_graceful_shutdown(async {
                // use a blocking task to avoid locking up the whole server
                // with this very synchronous channel recv process
                tokio::task::spawn_blocking(move || {
                    http_shutdown_recv
                        .recv()
                        .expect("failed to get http server shutdown signal");
                })
                .await
                .unwrap()
            });
            let http_join_handle =
                tokio::task::spawn(async move { http_future.await.expect("http server failed") });
            let url = format!("http2://{local_grpc_addr}").parse().unwrap();
            tracing::debug!("Connected to rpc test repo: {url}");
            let repo = spfs::storage::rpc::RpcRepository::from_url(&url)
                .await
                .unwrap()
                .into();
            TempRepo::Rpc {
                repo: Arc::new(repo),
                grpc_join_handle: Some(grpc_join_handle),
                http_join_handle: Some(http_join_handle),
                grpc_shutdown,
                http_shutdown,
                tmpdir,
            }
        }
        _ => panic!("unknown repo kind '{kind}'"),
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

/// Generate a random filesystem tree with a mix of folders and files
pub async fn generate_tree(tmprepo: &TempRepo) -> crate::tracking::Manifest {
    let tmpdir = tmpdir();

    let mut rng = rand::thread_rng();
    let max_depth = rng.gen_range(2..6);

    generate_subtree(tmpdir.path(), max_depth);
    crate::Committer::new(tmprepo)
        .commit_dir(tmpdir.path())
        .await
        .expect("Failed to commit generated tree")
}

fn generate_subtree(root: &std::path::Path, max_depth: i32) {
    let mut rng = rand::thread_rng();
    let dirs = rng.gen_range(2..6);
    let files = rng.gen_range(2..6);

    for _file in 0..files {
        let name_len = rng.gen_range(4..16);
        let name = Alphanumeric.sample_string(&mut rng, name_len);
        let data_len = rng.gen_range(8..64);
        let data = Alphanumeric.sample_string(&mut rng, data_len);
        std::fs::write(root.join(name), data).expect("Failed to generate file");
    }

    if max_depth > 1 {
        for _dir in 0..dirs {
            let name_len = rng.gen_range(4..16);
            let name = Alphanumeric.sample_string(&mut rng, name_len);
            let path = root.join(name);
            std::fs::create_dir_all(&path).expect("Failed to generate subdir");
            generate_subtree(&path, max_depth - 1);
        }
    }
}
