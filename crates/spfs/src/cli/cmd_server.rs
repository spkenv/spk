// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use structopt::StructOpt;

#[macro_use]
mod args;

main!(CmdServer);

#[derive(Debug, StructOpt)]
pub struct CmdServer {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        long = "remote",
        short = "r",
        about = "Serve a configured remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        long = "payloads-root",
        default_value = "http://localhost",
        about = "The external root url that clients can use to connect to this server"
    )]
    payloads_root: url::Url,
    // 7737 = spfs on a dial pad
    #[structopt(
        default_value = "0.0.0.0:7737",
        about = "The address to listen on for grpc requests"
    )]
    grpc_address: std::net::SocketAddr,
    #[structopt(
        default_value = "0.0.0.0:7787",
        about = "The address to listen on for http requests"
    )]
    http_address: std::net::SocketAddr,
}

impl CmdServer {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };
        let repo = std::sync::Arc::new(repo);

        let payload_service =
            spfs::server::PayloadService::new(repo.clone(), self.payloads_root.clone());
        let http_server = {
            let payload_service = payload_service.clone();
            hyper::Server::bind(&self.http_address).serve(hyper::service::make_service_fn(
                move |_| {
                    let s = payload_service.clone();
                    async move { Ok::<_, std::convert::Infallible>(s) }
                },
            ))
        };
        let http_future = http_server.with_graceful_shutdown(async {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(?err, "Failed to setup graceful shutdown handler");
            };
            tracing::info!("shutting down http server...");
        });
        let grpc_future = tonic::transport::Server::builder()
            .add_service(spfs::server::Repository::new_srv(repo.clone()))
            .add_service(spfs::server::TagService::new_srv(repo.clone()))
            .add_service(spfs::server::DatabaseService::new_srv(repo))
            .add_service(payload_service.into_srv())
            .serve_with_shutdown(self.grpc_address, async {
                if let Err(err) = tokio::signal::ctrl_c().await {
                    tracing::error!(?err, "Failed to setup graceful shutdown handler");
                };
                tracing::info!("shutting down gRPC server...");
            });
        tracing::info!("listening on: {}, {}", self.grpc_address, self.http_address);

        // TODO: stop the other server when one fails so that
        // the process can exit
        let (grpc_result, http_result) = tokio::join!(grpc_future, http_future,);
        if let Err(err) = grpc_result {
            tracing::error!("gRPC server failed: {:?}", err);
        }
        if let Err(err) = http_result {
            tracing::error!("http server failed: {:?}", err);
        }
        Ok(0)
    }
}
