// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use tracing_subscriber::prelude::*;

pub static SPFS_LOG: &str = "SPFS_LOG";

/// Command line flags for configuring sync operations
#[derive(Debug, Clone, clap::Args)]
pub struct Sync {
    /// Sync the latest information for each tag even if it already exists
    #[clap(short, long, alias = "pull")]
    pub sync: bool,

    /// Forcefully sync all associated graph data even if it
    /// already exists
    #[clap(long)]
    pub resync: bool,
}

impl Sync {
    /// Construct a new syncer instance configured based on these flags
    #[allow(dead_code)] // not all commands use this function but some do
    pub fn get_syncer<'src, 'dst>(
        &self,
        src: &'src spfs::storage::RepositoryHandle,
        dest: &'dst spfs::storage::RepositoryHandle,
    ) -> spfs::Syncer<'src, 'dst, spfs::sync::ConsoleSyncReporter> {
        spfs::Syncer::new(src, dest)
            .with_sync_existing_objects(self.resync)
            .with_sync_existing_payloads(self.resync)
            .with_sync_existing_tags(self.sync || self.resync)
            .with_reporter(spfs::sync::ConsoleSyncReporter::default())
    }
}

#[cfg(feature = "sentry")]
pub fn configure_sentry() {
    use sentry::IntoDsn;
    use std::borrow::Cow;
    let mut opts = sentry::ClientOptions {
        dsn: "http://3dd72e3b4b9a4032947304fabf29966e@sentry.k8s.spimageworks.com/4"
            .into_dsn()
            .unwrap_or(None),
        environment: Some(
            std::env::var("SENTRY_ENVIRONMENT")
                .unwrap_or_else(|_| "production".to_string())
                .into(),
        ),
        // spdev follows sentry recommendation of using the release
        // tag as the name of the release in sentry
        release: Some(format!("v{}", spfs::VERSION).into()),
        ..Default::default()
    };
    opts = sentry::apply_defaults(opts);

    // Proxy values may have been read from env.
    // If they do not contain a scheme prefix, sentry-transport
    // produces a panic log output
    if let Some(url) = opts.http_proxy.as_ref().map(ToString::to_string) {
        if !url.contains("://") {
            opts.http_proxy = Some(format!("http://{url}")).map(Cow::Owned);
        }
    }
    if let Some(url) = opts.https_proxy.as_ref().map(ToString::to_string) {
        if !url.contains("://") {
            opts.https_proxy = Some(format!("https://{url}")).map(Cow::Owned);
        }
    }

    let _guard = sentry::init(opts);

    sentry::configure_scope(|scope| {
        let username = whoami::username();
        scope.set_user(Some(sentry::protocol::User {
            email: Some(format!("{}@imageworks.com", &username)),
            username: Some(username),
            ..Default::default()
        }))
    })
}

pub fn configure_spops(_verbosity: usize) {
    // TODO: have something for this
    // try:
    //     spops.configure(
    //         {
    //             "statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111},
    //             "labels": {
    //                 "environment": os.getenv("SENTRY_ENVIRONMENT", "production"),
    //                 "user": getpass.getuser(),
    //                 "host": socket.gethostname(),
    //             },
    //         },
    //     )
    // except Exception as e:
    //     print(f"failed to initialize spops: {e}", file=sys.stderr)
}

pub fn configure_logging(verbosity: usize) {
    let mut config = match verbosity {
        0 => {
            if let Ok(existing) = std::env::var(SPFS_LOG) {
                existing
            } else {
                "spfs=info,warn".to_string()
            }
        }
        1 => "spfs=debug,info".to_string(),
        2 => "spfs=trace,info".to_string(),
        3 => "spfs=trace,debug".to_string(),
        _ => "trace".to_string(),
    };
    std::env::set_var(SPFS_LOG, &config);
    if let Ok(overrides) = std::env::var("RUST_LOG") {
        config.push(',');
        config.push_str(&overrides);
    }
    let env_filter = tracing_subscriber::filter::EnvFilter::from(config);
    let registry = tracing_subscriber::Registry::default().with(env_filter);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(verbosity > 2);
    let sub = registry.with(fmt_layer);
    tracing::subscriber::set_global_default(sub).unwrap();
}

#[macro_export]
macro_rules! main {
    ($cmd:ident) => {
        main!($cmd, sentry = true, sync = false);
    };
    ($cmd:ident, sentry = $sentry:literal, sync = true) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::parse();
            let config = configure!(opt, $sentry);

            let result = opt.run(&config);

            handle_result!(result)
        }
    };
    ($cmd:ident, sentry = $sentry:literal, sync = false) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::parse();
            let config = configure!(opt, $sentry);

            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Err(err) => {
                    tracing::error!("Failed to establish runtime: {:?}", err);
                    return 1;
                }
                Ok(rt) => rt,
            };
            let result = rt.block_on(opt.run(&config));
            // we generally expect at this point that the command is complete
            // and nothing else should be executing, but it's possible that
            // we've launched long running tasks that are waiting for signals or
            // events which will never come and so we don't want to block forever
            // when the runtime is dropped.
            rt.shutdown_timeout(std::time::Duration::from_millis(250));

            handle_result!(result)
        }
    };
}

macro_rules! configure {
    ($opt:ident, $sentry:literal) => {{
        // sentry makes this process multithreaded, and must be disabled
        // for commands that use system calls which are bothered by this
        #[cfg(feature = "sentry")]
        if $sentry { args::configure_sentry() }
        args::configure_logging($opt.verbose);
        args::configure_spops($opt.verbose);

        match spfs::get_config() {
            Err(err) => {
                tracing::error!(err = ?err, "failed to load config");
                return 1;
            }
            Ok(config) => config,
        }
    }};
}

macro_rules! handle_result {
    ($result:ident) => {
        match $result {
            Err(err) => {
                args::capture_if_relevant(&err);
                tracing::error!("{err}");
                1
            }
            Ok(code) => code,
        }
    };
}

pub fn capture_if_relevant(err: &spfs::Error) {
    match err {
        spfs::Error::NoActiveRuntime => (),
        spfs::Error::UnknownObject(_) => (),
        spfs::Error::UnknownReference(_) => (),
        spfs::Error::AmbiguousReference(_) => (),
        spfs::Error::NothingToCommit => (),
        _ => {
            #[cfg(features = "sentry")]
            sentry::capture_error(err);
        }
    }
}
