// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[cfg(feature = "sentry")]
use std::panic::catch_unwind;
#[cfg(feature = "sentry")]
use std::sync::Mutex;

use anyhow::Error;
#[cfg(feature = "sentry")]
use once_cell::sync::OnceCell;
use spfs::storage::LocalRepository;
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

/// Command line flags for configuring render operations
#[derive(Debug, Clone, clap::Args)]
pub struct Render {
    /// The total number of blobs that can be rendered concurrently
    #[clap(
        long,
        env = "SPFS_RENDER_MAX_CONCURRENT_BLOBS",
        default_value_t = spfs::storage::fs::DEFAULT_MAX_CONCURRENT_BLOBS
    )]
    pub max_concurrent_blobs: usize,

    /// The total number of branches that can be processed concurrently
    /// at each level of the rendered file tree.
    ///
    /// The number of active trees being processed can grow exponentially
    /// by this exponent for each additional level of depth in the rendered
    /// file tree. In general, this number should be kept low.
    #[clap(
        long,
        env = "SPFS_RENDER_MAX_CONCURRENT_BRANCHES",
        default_value_t = spfs::storage::fs::DEFAULT_MAX_CONCURRENT_BRANCHES
    )]
    pub max_concurrent_branches: usize,
}

impl Render {
    /// Construct a new renderer instance configured based on these flags
    #[allow(dead_code)] // not all commands use this function but some do
    pub fn get_renderer<'repo, Repo, Reporter>(
        &self,
        repo: &'repo Repo,
        reporter: Reporter,
    ) -> spfs::storage::fs::Renderer<'repo, Repo, Reporter>
    where
        Repo: spfs::storage::Repository + LocalRepository,
        Reporter: spfs::storage::fs::RenderReporter,
    {
        spfs::storage::fs::Renderer::new(repo)
            .with_max_concurrent_blobs(self.max_concurrent_blobs)
            .with_max_concurrent_branches(self.max_concurrent_branches)
            .with_reporter(reporter)
    }
}

#[cfg(feature = "sentry")]
fn get_cli_context(command: String) -> std::collections::BTreeMap<String, serde_json::Value> {
    use serde_json::json;

    let args: Vec<_> = std::env::args().collect();
    let program = args[0].clone();
    let cwd = std::env::current_dir().ok();

    let mut data = std::collections::BTreeMap::new();
    data.insert(String::from("program"), json!(program));
    data.insert(String::from("args"), json!(args.join(" ")));
    data.insert(String::from("cwd"), json!(cwd));
    data.insert(String::from("command"), json!(command));

    let env_vars_to_add = vec!["CI_JOB_URL"];
    for env_var in env_vars_to_add {
        if let Ok(value) = std::env::var(env_var) {
            data.insert(String::from(env_var), json!(value));
        }
    }

    data
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

// This is wrapped in `Mutex` to be able to explicitly drop the guard before
// exiting.
#[cfg(feature = "sentry")]
pub static SENTRY_GUARD: OnceCell<Option<Mutex<Option<sentry::ClientInitGuard>>>> = OnceCell::new();

#[cfg(feature = "sentry")]
pub fn configure_sentry(
    command: String,
) -> &'static Option<Mutex<Option<sentry::ClientInitGuard>>> {
    SENTRY_GUARD.get_or_init(|| {
        use std::borrow::Cow;

        use sentry::IntoDsn;

        // SENTRY_USERNAME_OVERRIDE_VAR should hold the name of another
        // environment variable that can hold a username. If it does and
        // the other environment variable exists, its value will be used
        // to override the username given to sentry events. This for sites
        // with automated processes triggered by humans, e.g. gitlab CI,
        // that run as a non-human user and store the original human's
        // username in an environment variable, e.g. GITLAB_USER_LOGIN.
        let username_override_var = option_env!("SENTRY_USERNAME_OVERRIDE_VAR");
        let username = username_override_var
            .map(std::env::var)
            .and_then(Result::ok)
            .unwrap_or_else(|| {
                // Call this before `sentry::init` to avoid possible data
                // race, SIGSEGV in `getpwuid_r ()` -> `getenv ()`. CentOS
                // 7.6.1810.  Thread 2 is always in `SSL_library_init ()` ->
                // `EVP_rc2_cbc ()`.
                whoami::username()
            });

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
            Ok(g) => {
                let data = get_cli_context(command.clone());

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

                Some(Mutex::new(Some(g)))
            }
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

        guard
    })
}

/// Drop the sentry guard if sentry has been initialized.
#[cfg(feature = "sentry")]
pub fn shutdown_sentry() {
    let Some(Some(mutex)) = SENTRY_GUARD.get() else { return };
    let Ok(mut opt_guard) = mutex.lock() else { return };
    // Steal the guard, if there was one, dropping it.
    opt_guard.take();
}

pub fn configure_logging(verbosity: usize, syslog: bool) {
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
    let fmt_layer = tracing_subscriber::fmt::layer()
        .without_time()
        .with_target(verbosity > 2);

    // TODO: Macro to DRY here?

    if syslog {
        let identity =
            std::ffi::CStr::from_bytes_with_nul(b"spfs\0").expect("identity value is valid CStr");
        let (options, facility) = Default::default();
        let syslog_log = fmt_layer.with_writer(
            syslog_tracing::Syslog::new(identity, options, facility).expect("initialize Syslog"),
        );

        #[cfg(not(feature = "sentry"))]
        let sub =
            tracing_subscriber::registry().with(syslog_log.with_filter(env_filter).with_filter(
                tracing_subscriber::filter::filter_fn(|metadata| {
                    // Don't log breadcrumbs to console, etc.
                    !metadata.target().starts_with("sentry")
                }),
            ));

        #[cfg(feature = "sentry")]
        let sub = {
            let sentry_layer =
                sentry_tracing::layer().with_filter(tracing_subscriber::filter::LevelFilter::INFO);

            tracing_subscriber::registry()
                .with(
                    syslog_log
                        .and_then(sentry_tracing::layer())
                        .with_filter(env_filter)
                        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                            // Don't log breadcrumbs to console, etc.
                            !metadata.target().starts_with("sentry")
                        })),
                )
                .with(
                    sentry_layer.with_filter(tracing_subscriber::filter::filter_fn(
                        // Only log breadcrumbs here.
                        |metadata| metadata.target().starts_with("sentry"),
                    )),
                )
        };

        tracing::subscriber::set_global_default(sub).unwrap();
    } else {
        let stderr_log = fmt_layer.with_writer(std::io::stderr);

        #[cfg(not(feature = "sentry"))]
        let sub =
            tracing_subscriber::registry().with(stderr_log.with_filter(env_filter).with_filter(
                tracing_subscriber::filter::filter_fn(|metadata| {
                    // Don't log breadcrumbs to console, etc.
                    !metadata.target().starts_with("sentry")
                }),
            ));

        #[cfg(feature = "sentry")]
        let sub = {
            let sentry_layer =
                sentry_tracing::layer().with_filter(tracing_subscriber::filter::LevelFilter::INFO);

            tracing_subscriber::registry()
                .with(
                    stderr_log
                        .and_then(sentry_tracing::layer())
                        .with_filter(env_filter)
                        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                            // Don't log breadcrumbs to console, etc.
                            !metadata.target().starts_with("sentry")
                        })),
                )
                .with(
                    sentry_layer.with_filter(tracing_subscriber::filter::filter_fn(
                        // Only log breadcrumbs here.
                        |metadata| metadata.target().starts_with("sentry"),
                    )),
                )
        };

        tracing::subscriber::set_global_default(sub).unwrap();
    };
}

/// Trait all spfs cli command parsers must implement to provide the
/// name of the spfs command that has been parsed. This method will be
/// called when configuring sentry.
pub trait CommandName {
    fn command_name(&self) -> &str;
}

#[macro_export]
macro_rules! main {
    ($cmd:ident) => {
        $crate::main!($cmd, sentry = true, sync = false, syslog = false);
    };
    ($cmd:ident, syslog = $syslog:literal) => {
        $crate::main!($cmd, sentry = true, sync = false, syslog = $syslog);
    };
    ($cmd:ident, sentry = $sentry:literal, sync = true) => {
        $crate::main!($cmd, sentry = $sentry, sync = true, syslog = false);
    };
    ($cmd:ident, sentry = $sentry:literal, sync = true, syslog = $syslog:literal) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::parse();
            let (config, sentry_guard) = $crate::configure!(opt, $sentry, $syslog);

            let result = opt.run(&config);

            $crate::handle_result!(result)
        }
    };
    ($cmd:ident, sentry = $sentry:literal, sync = false, syslog = $syslog:literal) => {
        fn main() {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2())
        }
        fn main2() -> i32 {
            let mut opt = $cmd::parse();
            let (config, sentry_guard) = $crate::configure!(opt, $sentry, $syslog);

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
    ($opt:ident, $sentry:literal, $syslog:literal) => {{
        // sentry makes this process multithreaded, and must be disabled
        // for commands that use system calls which are bothered by this
        #[cfg(feature = "sentry")]
        // TODO: pass $opt into sentry and into the get cli?
        let sentry_guard = if $sentry { $crate::configure_sentry(String::from($opt.command_name())) } else { &None };
        #[cfg(not(feature = "sentry"))]
        let sentry_guard = 0;
        $crate::configure_logging($opt.verbose, $syslog);

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
    ($result:ident) => {{
        let code = match $result {
            //  Err(err) => match err {
            Err(err) => match err.root_cause().downcast_ref::<spfs::Error>() {
                Some(spfs::Error::Errno(msg, errno))
                    if *errno == $crate::__private::libc::ENOSPC =>
                {
                    tracing::error!("Out of disk space: {msg}");
                    1
                }
                Some(spfs::Error::RuntimeWriteError(path, io_err))
                | Some(spfs::Error::StorageWriteError(_, path, io_err))
                    if std::matches!(
                        io_err.raw_os_error(),
                        Some($crate::__private::libc::ENOSPC)
                    ) =>
                {
                    tracing::error!("Out of disk space writing to {path}", path = path.display());
                    1
                }
                _ => {
                    $crate::capture_if_relevant(&err);
                    tracing::error!("{err}");
                    1
                }
            },
            Ok(code) => code,
        };

        // Explicitly consume the sentry guard here so it has a chance to
        // finish sending any pending events. The guard would not otherwise
        // get this chance because it is stored in a static.
        #[cfg(feature = "sentry")]
        $crate::shutdown_sentry();

        code
    }};
}

pub fn capture_if_relevant(err: &Error) {
    match err.root_cause().downcast_ref::<spfs::Error>() {
        Some(spfs::Error::NoActiveRuntime) => (),
        Some(spfs::Error::UnknownObject(_)) => (),
        Some(spfs::Error::UnknownReference(_)) => (),
        Some(spfs::Error::AmbiguousReference(_)) => (),
        Some(spfs::Error::NothingToCommit) => (),
        _ => {
            // This will always add a backtrace to the sentry event
            #[cfg(feature = "sentry")]
            sentry_anyhow::capture_anyhow(err);
        }
    }
}
