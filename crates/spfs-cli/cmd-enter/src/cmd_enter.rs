// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::ffi::OsString;
#[cfg(feature = "sentry")]
use std::sync::atomic::Ordering;
use std::time::Instant;

use clap::{Args, Parser};
#[cfg(feature = "sentry")]
use cli::configure_sentry;
use miette::{Context, Result};
#[cfg(unix)]
use spfs::monitor::SPFS_MONITOR_FOREGROUND_LOGGING_VAR;
use spfs::runtime::EnvKeyValue;
use spfs::storage::fs::RenderSummary;
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;
#[cfg(unix)]
use tokio::io::AsyncWriteExt;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
cli::main!(CmdEnter, sentry = false, sync = true);

/// Run a command in a configured spfs runtime
///
/// Although executable directly, this command is meant to be called
/// directly by other spfs commands in order to perform privileged
/// operations related to environment setup and teardown.
#[derive(Debug, Parser)]
#[clap(name = "spfs-enter")]
pub struct CmdEnter {
    #[clap(flatten)]
    pub logging: cli::Logging,

    #[clap(flatten)]
    make_durable: MakeDurableArgs,

    #[clap(flatten)]
    exit: ExitArgs,

    #[clap(flatten)]
    remount: RemountArgs,

    #[clap(flatten)]
    enter: EnterArgs,

    /// The address of the storage being used for runtimes
    ///
    /// Defaults to the current configured local repository.
    #[clap(long)]
    runtime_storage: Option<url::Url>,

    /// The name of the runtime
    #[clap(long)]
    runtime: String,
}

#[derive(Debug, Args)]
#[group(id = "durable_grp", conflicts_with_all = ["enter_grp", "exit_grp", "remount_grp"])]
pub struct MakeDurableArgs {
    /// Change the current runtime to a durable runtime
    #[clap(id = "make_durable", long = "make-durable")]
    enabled: bool,
}

#[derive(Debug, Args)]
#[group(id = "exit_grp", conflicts_with_all = ["durable_grp", "enter_grp", "remount_grp"])]
pub struct ExitArgs {
    /// Exit the current runtime, shutting down all filesystems
    #[clap(id = "exit", long = "exit")]
    enabled: bool,
}

#[derive(Debug, Args)]
#[group(id = "remount_grp", conflicts_with_all = ["durable_grp", "exit_grp", "enter_grp"])]
pub struct RemountArgs {
    /// Remount the overlay filesystem, don't enter a new namespace
    #[clap(id = "remount", long = "remount")]
    enabled: bool,
}

fn parse_env_key_value(s: &str) -> Result<EnvKeyValue> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        miette::bail!("Invalid environment key-value pair (missing '='): {s}");
    }
    if parts[0].is_empty() {
        miette::bail!("Invalid environment key-value pair (empty key): {s}");
    }
    if parts[0].contains(|c: char| c.is_whitespace()) {
        miette::bail!("Invalid environment key-value pair (key contains whitespace): {s}");
    }
    Ok(EnvKeyValue(parts[0].to_string(), parts[1].to_string()))
}

#[derive(Debug, Args)]
#[group(id = "enter_grp")]
pub struct EnterArgs {
    /// The value to set $TMPDIR to in new environment
    ///
    /// Deprecated: use --environment-override instead
    #[clap(long)]
    tmpdir: Option<String>,

    /// Optional keys and values to set in the new environment, in the form
    /// KEY=VALUE. This option can be repeated.
    #[clap(long, value_parser = parse_env_key_value)]
    environment_override: Vec<EnvKeyValue>,

    /// Put the rendering and syncing times into environment variables
    #[clap(long)]
    metrics_in_env: bool,

    /// The command to run after initialization
    ///
    /// If not given, run an interactive shell environment
    command: Option<OsString>,

    /// Additional arguments to provide to the command
    ///
    /// In order to ensure that flags are passed as-is, place '--' before
    /// specifying any flags that should be given to the subcommand:
    ///   eg `spfs enter <args> -- command --flag-for-command`
    args: Vec<OsString>,
}

impl CommandName for CmdEnter {
    fn command_name(&self) -> &'static str {
        "enter"
    }
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // we need a single-threaded runtime in order to properly setup
        // and enter the namespace of the runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                spfs::Error::String(format!("Failed to establish async runtime: {err:?}"))
            })?;

        #[cfg(target_os = "windows")]
        let setup_fut = { self.setup_runtime(config) };

        #[cfg(not(target_os = "windows"))]
        let setup_fut = {
            // Safety: we attempt to keep the process single threaded, so
            // setting environment variables is safe.
            unsafe { self.setup_runtime(config) }
        };

        let owned_runtime = rt.block_on(setup_fut)?;
        // do not block forever on drop because of any stuck blocking tasks
        rt.shutdown_timeout(std::time::Duration::from_millis(250));
        if let Some(rt) = owned_runtime {
            self.exec_runtime_command(rt)
        } else {
            Ok(0)
        }
    }

    #[cfg(unix)]
    /// Setup the runtime.
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub async unsafe fn setup_runtime(
        &mut self,
        config: &spfs::Config,
    ) -> Result<Option<spfs::runtime::OwnedRuntime>> {
        // this function will eventually be required to discover the overlayfs
        // attributes. It can take many milliseconds to run so we prime the cache as
        // soon as possible in a separate thread

        std::thread::spawn(spfs::runtime::overlayfs::overlayfs_available_options_prime_cache);

        let mut runtime = self.load_runtime(config).await?;

        tracing::debug!("entering runtime: {runtime:#?}");

        if self.make_durable.enabled {
            if runtime.is_durable() {
                return Err(spfs::Error::from("runtime is already durable").into());
            }
            let start_time = Instant::now();
            // Safety: the responsibility of the caller.
            let render_summary = unsafe { spfs::change_to_durable_runtime(&mut runtime).await? };
            // Safety: the responsibility of the caller.
            unsafe {
                self.report_render_summary(render_summary, start_time.elapsed().as_secs_f64())
            };
            tracing::info!("runtime remounted as durable");
            Ok(None)
        } else if self.exit.enabled {
            // Safety: the responsibility of the caller.
            let in_namespace =
                unsafe { spfs::env::RuntimeConfigurator::default().current_runtime(&runtime)? };
            let with_root = in_namespace.become_root()?;
            const LAZY: bool = false;
            with_root.unmount_env(&runtime, LAZY).await?;
            with_root.unmount_runtime(&runtime.config)?;
            with_root.become_original_user()?;
            Ok(None)
        } else if self.remount.enabled {
            let start_time = Instant::now();
            // Safety: the responsibility of the caller.
            let render_summary = unsafe { spfs::reinitialize_runtime(&mut runtime).await? };
            // Safety: the responsibility of the caller.
            unsafe {
                self.report_render_summary(render_summary, start_time.elapsed().as_secs_f64())
            };
            Ok(None)
        } else {
            let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_owner(runtime).await?;

            // Enter the mount namespace before spawning the monitor process
            // so that the monitor can properly view and manage that namespace.
            // For example, the monitor may need to run fusermount to clean up
            // fuse filesystems within the runtime before shutting down.
            tracing::debug!("initializing runtime {owned:#?}");

            let start_time = Instant::now();
            let render_summary = spfs::initialize_runtime(&mut owned).await?;
            // Safety: the responsibility of the caller.
            unsafe {
                self.report_render_summary(render_summary, start_time.elapsed().as_secs_f64())
            };

            let mut monitor_stdin = match spfs::monitor::spawn_monitor_for_runtime(&owned) {
                Err(err) => {
                    if !owned.is_durable() {
                        if let Err(err) = owned.delete().await {
                            tracing::error!(
                                ?err,
                                "failed to cleanup runtime data after failure to start monitor"
                            );
                        }
                    }
                    return Err(err.into());
                }
                Ok(mut child) => child.stdin.take().ok_or_else(|| {
                    spfs::Error::from("monitor was spawned without stdin attached")
                })?,
            };

            // We promise to not mutate the runtime after this point.
            // spfs-monitor will read and modify it after we tell it to
            // proceed.
            let owned = owned;

            // Now we have dropped privileges and are running as the invoking
            // user (same uid as spfs-monitor) and have entered the mount
            // namespace that spfs-monitor should be monitoring. Inform it to
            // proceed.
            tracing::debug!("informing spfs-monitor to proceed");
            let send_go = async move {
                monitor_stdin.write_all("go".as_bytes()).await?;
                monitor_stdin.flush().await?;
                Ok::<_, std::io::Error>(())
            }
            .await;
            match send_go {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                    // Pipe error generally means the spfs-monitor process is
                    // gone. If it failed to start/quit prematurely then it
                    // may have already deleted the runtime. It is not safe to
                    // proceed with using the runtime, so we don't ignore this
                    // error and hope for the best. Using this new environment
                    // puts whatever is using it at risk of data loss.
                    miette::bail!(
                        "spfs-monitor disappeared unexpectedly, it is unsafe to continue. Setting ${SPFS_MONITOR_FOREGROUND_LOGGING_VAR}=1 may provide more details"
                    );
                }
                Err(err) => {
                    miette::bail!("Failed to inform spfs-monitor to start: {err}");
                }
            };

            let mut environment_overrides = Cow::Borrowed(&self.enter.environment_override);
            if let Some(tmpdir) = &self.enter.tmpdir {
                environment_overrides
                    .to_mut()
                    .push(EnvKeyValue("TMPDIR".to_string(), tmpdir.to_string()));
            }

            owned.ensure_startup_scripts(environment_overrides.as_slice())?;
            // Safety: the responsibility of the caller.
            unsafe {
                std::env::set_var("SPFS_RUNTIME", owned.name());
            }

            Ok(Some(owned))
        }
    }

    #[cfg(windows)]
    pub async fn setup_runtime(
        &mut self,
        config: &spfs::Config,
    ) -> Result<Option<spfs::runtime::OwnedRuntime>> {
        let runtime = self.load_runtime(config).await?;
        tracing::debug!("entering runtime: {runtime:#?}");
        if self.exit.enabled {
            todo!()
        } else if self.remount.enabled {
            todo!()
        } else {
            let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_owner(runtime).await?;
            let start_time = Instant::now();
            let render_summary = spfs::initialize_runtime(&mut owned).await?;
            // Safety: it is documented to be safe to set environment variables
            // on Windows.
            unsafe {
                self.report_render_summary(render_summary, start_time.elapsed().as_secs_f64());
            }

            let mut environment_overrides = Cow::Borrowed(&self.enter.environment_override);
            if let Some(tmpdir) = &self.enter.tmpdir {
                environment_overrides
                    .to_mut()
                    .push(EnvKeyValue("TMPDIR".to_string(), tmpdir.to_string()));
            }

            owned.ensure_startup_scripts(environment_overrides.as_slice())?;
            // Safety: it is documented to be safe to set environment variables
            // on Windows.
            unsafe {
                std::env::set_var("SPFS_RUNTIME", owned.name());
            }

            Ok(Some(owned))
        }
    }

    async fn load_runtime(&self, config: &spfs::Config) -> Result<spfs::runtime::Runtime> {
        let repo = match &self.runtime_storage {
            Some(address) => spfs::open_repository(address).await?,
            None => config.get_local_repository_handle().await?,
        };
        let storage = spfs::runtime::Storage::new(repo)?;
        storage
            .read_runtime(&self.runtime)
            .await
            .map_err(|err| err.into())
    }

    fn exec_runtime_command(&mut self, rt: spfs::runtime::OwnedRuntime) -> Result<i32> {
        let cmd = match self.enter.command.take() {
            Some(exe) if !exe.is_empty() => {
                tracing::debug!("executing runtime command");
                spfs::build_shell_initialized_command(&rt, None, exe, self.enter.args.drain(..))?
            }
            _ => {
                tracing::debug!("starting interactive shell environment");
                spfs::build_interactive_shell_command(&rt, None)?
            }
        };

        cmd.exec()
            .map(|_| 0)
            .wrap_err("Failed to execute runtime command")
    }

    #[cfg_attr(not(feature = "sentry"), allow(unused))]
    /// Report the render summary to sentry
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    unsafe fn report_render_summary(&self, render_summary: RenderSummary, render_time: f64) {
        if self.enter.metrics_in_env {
            // The render time is put into environment variables for
            // other, non-spfs, programs to access. If this command
            // has been run from spfs-run, then the sync time will
            // already be in an environment variable called
            // SPFS_METRICS_SYNC_TIME_SECS as well.
            // Safety: the responsibility of the caller.
            unsafe {
                std::env::set_var("SPFS_METRICS_RENDER_TIME_SECS", format!("{render_time}"));
            }
        }

        #[cfg(feature = "sentry")]
        {
            // Don't log if nothing was rendered.
            if render_summary.is_zero() {
                return;
            }
            // This is called after `[re]initialize_runtime` and now it is
            // "safe" to initialize sentry and send a sentry event.
            if let Some(_guard) = configure_sentry(self.command_name().to_owned()) {
                // Sync time is read in so it can be added to the
                // sentry data. It's not being used for anything else
                // so it doesn't need to be converted to a float.
                let sync_time =
                    std::env::var("SPFS_METRICS_SYNC_TIME_SECS").unwrap_or(String::from("0.0"));

                tracing::error!(
                        target: "sentry",
                        entry_count = %render_summary.entry_count.load(Ordering::Relaxed),
                        already_existed_count = %render_summary.already_existed_count.load(Ordering::Relaxed),
                        copy_count = %render_summary.copy_count.load(Ordering::Relaxed),
                        copy_link_limit_count = %render_summary.copy_link_limit_count.load(Ordering::Relaxed),
                        copy_wrong_mode_count = %render_summary.copy_wrong_mode_count.load(Ordering::Relaxed),
                        copy_wrong_owner_count = %render_summary.copy_wrong_owner_count.load(Ordering::Relaxed),
                        link_count = %render_summary.link_count.load(Ordering::Relaxed),
                        symlink_count = %render_summary.symlink_count.load(Ordering::Relaxed),
                        total_bytes_rendered = %render_summary.total_bytes_rendered.load(Ordering::Relaxed),
                        total_bytes_already_existed = %render_summary.total_bytes_already_existed.load(Ordering::Relaxed),
                        total_bytes_copied = %render_summary.total_bytes_copied.load(Ordering::Relaxed),
                        total_bytes_copied_link_limit = %render_summary.total_bytes_copied_link_limit.load(Ordering::Relaxed),
                        total_bytes_copied_wrong_mode = %render_summary.total_bytes_copied_wrong_mode.load(Ordering::Relaxed),
                        total_bytes_copied_wrong_owner = %render_summary.total_bytes_copied_wrong_owner.load(Ordering::Relaxed),
                        total_bytes_linked = %render_summary.total_bytes_linked.load(Ordering::Relaxed),
                        sync_time = %sync_time,
                        render_time = %render_time,
                        "Render summary");
            }
        }
    }
}
