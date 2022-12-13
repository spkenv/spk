// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[cfg(feature = "sentry")]
use std::panic::catch_unwind;

use tracing_subscriber::prelude::*;

const SPFS_LOG: &str = "SPFS_LOG";

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

    /// The total number of manifests that can be synced concurrently
    #[clap(
        long,
        env = "SPFS_SYNC_MAX_CONCURRENT_MANIFESTS",
        default_value_t = spfs::sync::DEFAULT_MAX_CONCURRENT_MANIFESTS
    )]
    pub max_concurrent_manifests: usize,

    /// The total number of file payloads that can be synced concurrently
    #[clap(
        long,
        env = "SPFS_SYNC_MAX_CONCURRENT_PAYLOADS",
        default_value_t = spfs::sync::DEFAULT_MAX_CONCURRENT_PAYLOADS
    )]
    pub max_concurrent_payloads: usize,
}

impl Sync {
    /// Construct a new syncer instance configured based on these flags
    #[allow(dead_code)] // not all commands use this function but some do
    pub fn get_syncer<'src, 'dst>(
        &self,
        src: &'src spfs::storage::RepositoryHandle,
        dest: &'dst spfs::storage::RepositoryHandle,
    ) -> spfs::Syncer<'src, 'dst, spfs::sync::ConsoleSyncReporter> {
        let policy = if self.resync {
            spfs::sync::SyncPolicy::ResyncEverything
        } else if self.sync {
            spfs::sync::SyncPolicy::LatestTags
        } else {
            spfs::sync::SyncPolicy::default()
        };
        spfs::Syncer::new(src, dest)
            .with_policy(policy)
            .with_max_concurrent_manifests(self.max_concurrent_manifests)
            .with_max_concurrent_payloads(self.max_concurrent_payloads)
            .with_reporter(spfs::sync::ConsoleSyncReporter::default())
    }
}

#[cfg(feature = "sentry")]
fn get_cli_context() -> (
    String,
    std::collections::BTreeMap<String, serde_json::Value>,
) {
    use serde_json::json;

    let args: Vec<_> = std::env::args().collect();
    let program = args[0].clone();
    let command = args[1].clone();
    let cwd = std::env::current_dir().ok();

    let mut data = std::collections::BTreeMap::new();
    data.insert(String::from("program"), json!(program));
    data.insert(String::from("command"), json!(command));
    data.insert(String::from("args"), json!(args.join(" ")));
    data.insert(String::from("cwd"), json!(cwd));

    (command, data)
}

#[cfg(feature = "sentry")]
fn remove_ansi_escapes(message: String) -> String {
    if let Ok(b) = strip_ansi_escapes::strip(message.clone()) {
        if let Ok(s) = std::str::from_utf8(&b) {
            return s.to_string();
        }
    }
    message
}

#[cfg(feature = "sentry")]
pub fn configure_sentry() -> Option<sentry::ClientInitGuard> {
    use std::borrow::Cow;

    use sentry::IntoDsn;

    // Call this before `sentry::init` to avoid possible data race, SIGSEGV
    // in `getpwuid_r ()` -> `getenv ()`. CentOS 7.6.1810.
    // Thread 2 is always in `SSL_library_init ()` -> `EVP_rc2_cbc ()`.
    let username = whoami::username();

    let guard = match catch_unwind(|| {
        let mut opts = sentry::ClientOptions {
            dsn: "http://3dd72e3b4b9a4032947304fabf29966e@sentry.spimageworks.com/4"
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
            before_send: Some(std::sync::Arc::new(|mut event| {
                // Remove ansi color codes from the event message
                if let Some(message) = event.message {
                    event.message = Some(remove_ansi_escapes(message));
                }
                Some(event)
            })),
            before_breadcrumb: Some(std::sync::Arc::new(|mut breadcrumb| {
                // Remove ansi color codes from the breadcrumb message
                if let Some(message) = breadcrumb.message {
                    breadcrumb.message = Some(remove_ansi_escapes(message));
                }
                Some(breadcrumb)
            })),
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

        sentry::init(opts)
    }) {
        Ok(g) => Some(g),
        Err(cause) => {
            // Added to try to get more info on this kind of panic:
            //
            // thread 'main' panicked at 'called `Result::unwrap()` on
            // an `Err` value: Os { code: 11, kind: WouldBlock,
            // message: "Resource temporarily unavailable" }',
            // /.../sentry-core-0.27.0/src/session.rs:228:14
            //
            // See also, maybe?: https://github.com/rust-lang/rust/issues/46345
            eprintln!("WARNING: configuring Sentry for spfs failed: {:?}", cause);
            None
        }
    };

    let (command, data) = get_cli_context();

    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::protocol::User {
            // TODO: make this configurable in future
            email: Some(format!("{}@imageworks.com", &username)),
            username: Some(username),
            ..Default::default()
        }));

        // Tags are searchable
        scope.set_tag("command", command);
        // Contexts are not searchable
        scope.set_context("SPFS", sentry::protocol::Context::Other(data));
    });

    guard
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

    #[cfg(not(feature = "sentry"))]
    let sub = registry.with(fmt_layer);

    #[cfg(feature = "sentry")]
    let sub = registry.with(fmt_layer).with(sentry_tracing::layer());

    tracing::subscriber::set_global_default(sub).unwrap();
}

#[macro_export]
macro_rules! main {
    ($cmd:ident) => {
        $crate::main!($cmd, sentry = true, sync = false);
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
            let (config, sentry_guard) = $crate::configure!(opt, $sentry);

            let result = opt.run(&config);

            $crate::handle_result!(result)
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
            let (config, sentry_guard) = $crate::configure!(opt, $sentry);

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

            $crate::handle_result!(result)
        }
    };
}

#[macro_export(local_inner_macros)]
macro_rules! configure {
    ($opt:ident, $sentry:literal) => {{
        // sentry makes this process multithreaded, and must be disabled
        // for commands that use system calls which are bothered by this
        #[cfg(feature = "sentry")]
        let sentry_guard = if $sentry { $crate::configure_sentry() } else { None };
        #[cfg(not(feature = "sentry"))]
        let sentry_guard = 0;
        $crate::configure_logging($opt.verbose);
        $crate::configure_spops($opt.verbose);

        match spfs::get_config() {
            Err(err) => {
                tracing::error!(err = ?err, "failed to load config");
                return 1;
            }
            Ok(config) => (config, sentry_guard),
        }
    }};
}

#[macro_export(local_inner_macros)]
macro_rules! handle_result {
    ($result:ident) => {
        match $result {
            Err(err) => {
                $crate::capture_if_relevant(&err);
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
            #[cfg(feature = "sentry")]
            sentry::capture_error(err);
        }
    }
}
