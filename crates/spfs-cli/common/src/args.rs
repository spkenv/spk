// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(feature = "sentry")]
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "sentry")]
use std::sync::Mutex;

use miette::{Error, IntoDiagnostic, Result, WrapErr};
#[cfg(feature = "sentry")]
use once_cell::sync::OnceCell;
use spfs::io::Pluralize;
use spfs::storage::LocalRepository;
use tracing_subscriber::prelude::*;

const SPFS_LOG: &str = "SPFS_LOG";

/// Options for showing progress
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum Progress {
    /// Show progress bars (default)
    #[default]
    Bars,
    /// Do not show any progress
    None,
}

/// Command line flags for configuring sync operations
#[derive(Debug, Clone, clap::Args)]
pub struct Sync {
    /// Sync the latest information for each tag even if it already exists
    #[clap(long, alias = "pull")]
    pub sync: bool,

    /// Traverse and check the entire graph, filling in any missing data
    ///
    /// When a repository is in good health, this should not be necessary, but
    /// if some subset of the data has been deleted or lost, this option may
    /// help recover it.
    #[clap(long)]
    pub check: bool,

    /// Forcefully sync all associated graph data even if it already exists
    ///
    /// When a repository is in good health, this should not be necessary, but
    /// if some subset of the data has been deleted, lost, or corrupted this
    /// option may help recover it.
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

    /// Options for showing progress
    #[clap(long, value_enum)]
    pub progress: Option<Progress>,
}

impl Sync {
    /// Construct a new syncer instance configured based on these flags
    #[allow(dead_code)] // not all commands use this function but some do
    pub fn get_syncer<'src, 'dst>(
        &self,
        src: &'src spfs::storage::RepositoryHandle,
        dest: &'dst spfs::storage::RepositoryHandle,
    ) -> spfs::Syncer<'src, 'dst> {
        let policy = self.sync_policy();
        let syncer = spfs::Syncer::new(src, dest)
            .with_policy(policy)
            .with_max_concurrent_manifests(self.max_concurrent_manifests)
            .with_max_concurrent_payloads(self.max_concurrent_payloads);

        match self.progress.unwrap_or_default() {
            Progress::Bars => syncer.with_reporter(spfs::sync::reporter::SyncReporters::console()),
            Progress::None => syncer,
        }
    }

    /// The selected sync policy for these options
    pub fn sync_policy(&self) -> spfs::sync::SyncPolicy {
        if self.resync {
            spfs::sync::SyncPolicy::ResyncEverything
        } else if self.check {
            spfs::sync::SyncPolicy::LatestTagsAndResyncObjects
        } else if self.sync {
            spfs::sync::SyncPolicy::LatestTags
        } else {
            spfs::sync::SyncPolicy::default()
        }
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
    let b = strip_ansi_escapes::strip(&message);
    if let Ok(s) = std::str::from_utf8(&b) {
        return s.to_string();
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
) -> Option<&'static Mutex<Option<sentry::ClientInitGuard>>> {
    SENTRY_GUARD
        .get_or_init(|| {
            use std::borrow::Cow;

            use sentry::IntoDsn;

            let Ok(config) = spfs::get_config() else {
                return None;
            };

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

            let sentry_init_result = catch_unwind(|| {
                let mut opts = sentry::ClientOptions {
                    dsn: config.sentry.dsn.as_str().into_dsn().unwrap_or_default(),
                    environment: config
                        .sentry
                        .environment
                        .as_ref()
                        .map(ToOwned::to_owned)
                        .map(std::borrow::Cow::Owned),
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
                if let Some(url) = opts.http_proxy.as_ref().map(ToString::to_string)
                    && !url.contains("://")
                {
                    opts.http_proxy = Some(format!("http://{url}")).map(Cow::Owned);
                }

                if let Some(url) = opts.https_proxy.as_ref().map(ToString::to_string)
                    && !url.contains("://")
                {
                    opts.https_proxy = Some(format!("https://{url}")).map(Cow::Owned);
                }

                sentry::init(opts)
            });

            match sentry_init_result {
                Ok(g) => {
                    let data = get_cli_context(command.clone());

                    sentry::configure_scope(|scope| {
                        scope.set_user(Some(sentry::protocol::User {
                            email: config
                                .sentry
                                .email_domain
                                .as_ref()
                                .map(|domain| format!("{username}@{domain}")),
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
            }
        })
        .as_ref()
}

/// Drop the sentry guard if sentry has been initialized.
#[cfg(feature = "sentry")]
pub fn shutdown_sentry() {
    let Some(Some(mutex)) = SENTRY_GUARD.get() else {
        return;
    };
    let Ok(mut opt_guard) = mutex.lock() else {
        return;
    };
    // Steal the guard, if there was one, dropping it.
    opt_guard.take();
}

/// Command line flags for configuring logging and output
#[derive(Debug, Clone, clap::Args)]
pub struct Logging {
    /// Make output more verbose, can be specified more than once
    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Additionally log output to the provided file
    #[clap(long, global = true, env = "SPFS_LOG_FILE", value_hint = clap::ValueHint::FilePath)]
    pub log_file: Option<std::path::PathBuf>,

    /// Enables logging to syslog (for background processes, unix only)
    #[clap(skip)]
    pub syslog: bool,

    /// Enables timestamp in logging (always enabled in file log)
    #[clap(long, global = true, value_parser = clap::builder::BoolishValueParser::new(), env = "SPFS_LOG_TIMESTAMP")]
    pub timestamp: bool,
}

/// Applies a filter to remove sentry log targets if sentry is enabled
macro_rules! without_sentry_target {
    ($layer:ident) => {{
        $layer.with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            // Don't log breadcrumbs to console, etc.
            !metadata.target().starts_with("sentry")
        }))
    }};
}

macro_rules! configure_timestamp {
    ($tracing_layer:expr, $timestamp:expr) => {
        if $timestamp {
            $tracing_layer.boxed()
        } else {
            $tracing_layer.without_time().boxed()
        }
    };
}

impl Logging {
    fn show_target(&self) -> bool {
        self.verbose > 2
    }

    /// Configure logging based on the command line flags.
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub unsafe fn configure(&self) {
        let mut config = match self.verbose {
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
        // Safety: the responsibility of the caller.
        unsafe {
            std::env::set_var(SPFS_LOG, &config);
        }
        if let Ok(overrides) = std::env::var("RUST_LOG") {
            config.push(',');
            config.push_str(&overrides);
        }

        let env_filter = move || tracing_subscriber::filter::EnvFilter::from(config.clone());
        let fmt_layer = || tracing_subscriber::fmt::layer().with_target(self.show_target());

        #[cfg(unix)]
        let syslog_layer = self.syslog.then(|| {
            let identity = c"spfs";
            let (options, facility) = Default::default();
            let layer = fmt_layer().with_writer(
                syslog_tracing::Syslog::new(identity, options, facility)
                    .expect("initialize Syslog"),
            );
            let layer = configure_timestamp!(layer, self.timestamp).with_filter(env_filter());
            without_sentry_target!(layer)
        });
        #[cfg(windows)]
        let syslog_layer = false.then(fmt_layer);

        let stderr_layer = {
            let layer = fmt_layer().with_writer(std::io::stderr);
            let layer = configure_timestamp!(layer, self.timestamp).with_filter(env_filter());
            without_sentry_target!(layer)
        };

        let file_layer = self
            .log_file
            .as_ref()
            .and_then(|log_file_path| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(log_file_path)
                    .ok()
            })
            .map(|log_file| {
                let layer = fmt_layer().with_writer(log_file);
                let layer = configure_timestamp!(layer, {
                    // file logs should always have a timestamp (fight me!)
                    true
                })
                .with_filter(env_filter());
                without_sentry_target!(layer)
            });

        #[cfg(feature = "sentry")]
        let sentry_layer = Some(
            sentry_tracing::layer().with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        );
        #[cfg(not(feature = "sentry"))]
        let sentry_layer = false.then(fmt_layer);

        tracing_subscriber::Layer::and_then(sentry_layer, file_layer)
            .and_then(syslog_layer)
            .and_then(stderr_layer)
            .with_subscriber(tracing_subscriber::Registry::default())
            .init();
    }
}

/// Log a message at the warning level and also generate a sentry event if
/// sentry is enabled.
#[macro_export]
macro_rules! warn_and_sentry_event {
    ($($arg:tt)*) => {
        #[cfg(feature = "sentry")]
        {
            tracing::error!(target: "sentry", $($arg)*);
        }
        tracing::warn!($($arg)*);
    };
}

/// Command line flags for viewing annotations in a runtime
#[derive(Debug, Clone, clap::Args)]
pub struct AnnotationViewing {
    /// Output the data value for the given annotation key(s) from
    /// the active runtime. Each value is printed on its own line
    /// without its key.
    #[clap(long, alias = "annotation")]
    pub get: Option<Vec<String>>,

    /// Output all the annotation keys and values from the active
    /// runtime as a yaml dictionary
    #[clap(long, alias = "all-annotations")]
    pub get_all: bool,
}

impl AnnotationViewing {
    /// Display annotation values based on the command line arguments
    pub async fn print_data(&self, runtime: &spfs::runtime::Runtime) -> Result<()> {
        if self.get_all {
            let data = runtime.all_annotations().await?;
            let keys = data
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<String>>();
            let num_keys = keys.len();
            tracing::debug!(
                "{num_keys} annotation {}: {}",
                "key".pluralize(num_keys),
                keys.join(", ")
            );
            println!(
                "{}",
                serde_yaml::to_string(&data)
                    .into_diagnostic()
                    .wrap_err("Failed to generate yaml output")?
            );
        } else if let Some(keys) = &self.get {
            tracing::debug!("--get these keys: {}", keys.join(", "));
            for key in keys.iter() {
                match runtime.annotation(key).await? {
                    Some(value) => {
                        tracing::debug!("{key} = {value}");
                        println!("{value}");
                    }
                    None => {
                        tracing::warn!("No annotation stored under: {key}");
                        println!();
                    }
                }
            }
        }

        Ok(())
    }
}

/// Command line flags for repository selection and setup
#[derive(Debug, Clone, clap::Args)]
pub struct Repositories {
    /// Operate on a remote repository instead of the local one
    ///
    /// This is really only helpful if you are providing a specific ref to look up.
    #[clap(long, short)]
    pub remote: Option<String>,

    /// Add the path as a spfs filesystem repo that wraps the existing
    /// 'origin' remote repo inside a proxy repo.
    ///
    /// The repo at the given filepath becomes the primary repo in an
    /// 'origin' proxy repo that wraps the original origin repo as the
    /// proxy's secondary repo. If things aren't found in the primary
    /// repo, it will fall through to the secondary repo.
    ///
    /// This allows repos that are not in the normal config files to
    /// be interacted with by individual commands, such as siloed
    /// per-job or per-show repos.
    #[clap(long, alias = "insert-proxy-repo")]
    pub add_proxy_repo: Option<PathBuf>,
}

/// Trait all spfs cli command parsers must implement to allow extra
/// repos to be configured on the command line. This method will be
/// called when configuring the program being run.
pub trait HasRepositoryArgs {
    fn configure_repositories_from_args(
        &self,
        config: Arc<spfs::Config>,
    ) -> Result<Arc<spfs::Config>> {
        // Does nothing by default. Some commands will override this
        // to return an updated config based on their Repositories
        // command line options, if any.
        Ok(config)
    }
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
        fn main() -> miette::Result<()> {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2()?);
        }
        fn main2() -> miette::Result<i32> {
            let mut opt = $cmd::parse();
            let (config, sentry_guard) = $crate::configure!(opt, $sentry, $syslog);

            let result = opt.run(&config);

            $crate::handle_result!(result)
        }
    };
    ($cmd:ident, sentry = $sentry:literal, sync = false, syslog = $syslog:literal) => {
        fn main() -> miette::Result<()> {
            // because this function exits right away it does not
            // properly handle destruction of data, so we put the actual
            // logic into a separate function/scope
            std::process::exit(main2()?);
        }
        fn main2() -> miette::Result<i32> {
            let mut opt = $cmd::parse();
            let (config, sentry_guard) = $crate::configure!(opt, $sentry, $syslog);

            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Err(err) => {
                    tracing::error!("Failed to establish runtime: {:?}", err);
                    return Ok(1);
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
        use $crate::CommandName;
        // sentry makes this process multithreaded, and must be disabled
        // for commands that use system calls which are bothered by this
        #[cfg(feature = "sentry")]
        // TODO: pass $opt into sentry and into the get cli?
        let sentry_guard = if $sentry { $crate::configure_sentry(String::from($opt.command_name())) } else { None };
        #[cfg(not(feature = "sentry"))]
        let sentry_guard = ();
        $opt.logging.syslog = $syslog;
        // Safety: unless sentry is enabled, the process is single threaded
        // still and it is safe to set environment variables.
        unsafe {
            $opt.logging.configure();
        }

        match spfs::get_config() {
            Err(err) => {
                tracing::error!(err = ?err, "failed to load config");
                return Ok(1);
            }
            Ok(config) => {
                match $opt.configure_repositories_from_args(config.clone()) {
                    Err(err) => {
                        tracing::error!(err = ?err, "failed to update config from cli args");
                        return Ok(2);
                    }
                    Ok(updated_config) => (updated_config, sentry_guard),
                }
            },
        }
    }};
}

#[macro_export(local_inner_macros)]
macro_rules! handle_result {
    ($result:ident) => {{
        use $crate::__private::spfs::OsError;
        let res = match $result {
            Err(err) => match err.root_cause().downcast_ref::<spfs::Error>() {
                Some(spfs::Error::Errno(msg, errno))
                    if *errno == $crate::__private::libc::ENOSPC =>
                {
                    tracing::error!("Out of disk space: {msg}");
                    Ok(1)
                }
                Some(spfs::Error::RuntimeWriteError(path, io_err))
                | Some(spfs::Error::StorageWriteError(_, path, io_err))
                    if std::matches!(io_err.os_error(), Some($crate::__private::libc::ENOSPC)) =>
                {
                    tracing::error!("Out of disk space writing to {path}", path = path.display());
                    Ok(1)
                }
                _ => {
                    $crate::capture_if_relevant(&err);
                    Err(err)
                }
            },
            Ok(code) => Ok(code),
        };

        // Explicitly consume the sentry guard here so it has a chance to
        // finish sending any pending events. The guard would not otherwise
        // get this chance because it is stored in a static.
        #[cfg(feature = "sentry")]
        $crate::shutdown_sentry();

        res
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
            #[cfg(feature = "sentry")]
            sentry_miette::capture_miette(err);
        }
    }
}
