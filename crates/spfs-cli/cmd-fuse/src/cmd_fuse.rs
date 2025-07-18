// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::time::Duration;

use clap::Parser;
#[cfg(unix)]
use fuser::MountOption;
use miette::Result;
#[cfg(unix)]
use miette::{Context, IntoDiagnostic, bail, miette};
#[cfg(unix)]
use spfs::Error;
use spfs::tracking::EnvSpec;
#[cfg(unix)]
use spfs_cli_common::warn_and_sentry_event;
use spfs_cli_common::{self as cli};
#[cfg(unix)]
use spfs_vfs::{Config, Session};
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
#[cfg(unix)]
use tokio::time::timeout;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
fn main() {
    // because this function exits right away it does not
    // properly handle destruction of data, so we put the actual
    // logic into a separate function/scope
    std::process::exit(main2())
}
fn main2() -> i32 {
    let mut opt = CmdFuse::parse();
    opt.logging
        .log_file
        .get_or_insert("/tmp/spfs-runtime/fuse.log".into());
    opt.logging.syslog = true;
    // Safety: the process is single threaded still and it is safe to set
    // environment variables.
    unsafe {
        opt.logging.configure();
    }

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return 1;
        }
        Ok(config) => config,
    };
    let result = opt.run(&config);
    let result = spfs_cli_common::handle_result!(result);
    // a regular main function that returns an error prints
    // that message to stdout. Because there is rarely
    // any way to view stderr for this mount process,
    // we explicitly log the error to tracing so that
    // it will appear in the fuse log and syslog.
    match result {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{err:?}");
            1
        }
    }
}

/// Run a fuse
#[derive(Debug, Parser)]
#[clap(name = "spfs-fuse")]
pub struct CmdFuse {
    #[clap(flatten)]
    logging: cli::Logging,

    /// Do not daemonize the filesystem, run it in the foreground instead
    #[clap(long, short)]
    foreground: bool,

    /// Do not disconnect the filesystem logs from stderr
    ///
    /// Although the filesystem will still daemonize, the logs will
    /// still appear in the stderr of the calling process/shell
    #[clap(long, short, env = "SPFS_FUSE_LOG_FOREGROUND")]
    log_foreground: bool,

    /// Options for the mount in the form opt1,opt2=value
    ///
    /// In addition to all existing fuse mount options, the following custom
    /// options are also supported:
    ///
    ///  uid    - the user id that should own all files in the mount, defaults to
    ///           the effective user id of the caller. Only allowed when running
    ///           as root/sudo.
    ///  gid    - the group id that should own all files in the mount, defaults to
    ///           the effective user id of the caller. Only allowed when running
    ///           as root/sudo.
    ///  remote - additional remote repository to read data from, can be given more
    ///           than once
    #[clap(long, short, value_delimiter = ',')]
    options: Vec<String>,

    /// The tag or id of the files to mount
    ///
    /// Use '-' or an empty string to request an empty environment
    #[clap(name = "REF")]
    reference: EnvSpec,

    /// The location where to mount the spfs runtime
    #[clap(default_value = "/spfs")]
    mountpoint: std::path::PathBuf,
}

impl cli::CommandName for CmdFuse {
    fn command_name(&self) -> &str {
        "fuse"
    }
}

impl CmdFuse {
    #[cfg(unix)]
    pub fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let calling_uid = nix::unistd::geteuid();
        let calling_gid = nix::unistd::getegid();

        // these will cause conflicts later on if their counterpart is also provided
        let mut required_opts = vec![MountOption::RO, MountOption::NoDev, MountOption::NoSuid];
        if !fuse3_available() {
            // the nonempty option became a default and was removed in
            // fuse3 but is still needed for fuse2
            required_opts.push(MountOption::CUSTOM("nonempty".into()));
        }
        let mut opts = Config {
            root_mode: 0o777,
            uid: calling_uid,
            gid: calling_gid,
            remotes: Vec::new(),
            mount_options: required_opts.into_iter().collect(),
            include_secondary_tags: config.fuse.include_secondary_tags,
        };

        let parsed_opts = parse_options_from_args(&self.options);
        for option in parsed_opts {
            match option {
                MountOption::CUSTOM(opt) => {
                    match opt.split_once('=') {
                        Some(("remote", name)) => {
                            opts.remotes.push(name.to_owned());
                        }
                        Some(("incl_sec_tags", value)) => {
                            opts.include_secondary_tags = value.parse::<bool>().map_err(|err| {
                                Error::String(format!(
                                    "Invalid parameter value for incl_sec_tags={value}: {err}"
                                ))
                            })?
                        }
                        Some(("uid", num)) if calling_uid.is_root() => {
                            opts.uid = num.parse::<u32>().map(nix::unistd::Uid::from_raw).map_err(
                                |err| {
                                    Error::String(format!(
                                        "Invalid parameter value for uid={num}: {err}"
                                    ))
                                },
                            )?
                        }
                        Some(("gid", num)) if calling_uid.is_root() => {
                            opts.gid = num.parse::<u32>().map(nix::unistd::Gid::from_raw).map_err(
                                |err| {
                                    Error::String(format!(
                                        "Invalid parameter value for gid={num}: {err}"
                                    ))
                                },
                            )?
                        }
                        Some(("uid", _)) | Some(("gid", _)) => {
                            bail!("Must be root to launch with alternate uid/gid");
                        }
                        _ => bail!("Unsupported mount option, or missing value: {opt}"),
                    }
                }
                _ => {
                    opts.mount_options.insert(option);
                }
            }
        }

        tracing::debug!("FUSE Config: {opts:#?}");

        if opts.mount_options.contains(&MountOption::RW) {
            bail!("rw mode is not supported, yet");
        }

        let mountpoint = self
            .mountpoint
            .canonicalize()
            .into_diagnostic()
            .wrap_err("Invalid mount point")?;

        if !opts.uid.is_root() {
            nix::unistd::seteuid(opts.uid)
                .into_diagnostic()
                .wrap_err("Failed to become desired user (effective)")?;
            nix::unistd::setegid(opts.gid)
                .into_diagnostic()
                .wrap_err("Failed to become desired group (effective)")?;
            // unprivileged users must have write access to the directory that
            // they are trying to mount over.
            nix::unistd::access(&mountpoint, nix::unistd::AccessFlags::W_OK)
                .into_diagnostic()
                .wrap_err("Must have write access to mountpoint")?;
            nix::unistd::seteuid(calling_uid)
                .into_diagnostic()
                .wrap_err("Failed to reset calling user (effective)")?;
            nix::unistd::setegid(calling_gid)
                .into_diagnostic()
                .wrap_err("Failed to reset calling group (effective)")?;
        }

        // establish the fuse session before changing the uid/gid of this process
        // so that we are allowed to use options such as `allow_other`. We also
        // need root to have access to this mount so that it can be properly
        // introspected and unmounted by other parts of spfs such as the monitor
        tracing::debug!("Establishing fuse session...");
        let mount_opts = opts.mount_options.iter().cloned().collect::<Vec<_>>();
        let session = Session::new(self.reference.clone(), opts.clone());
        let mut fuser_session = fuser::Session::new(session.clone(), &mountpoint, &mount_opts)
            .into_diagnostic()
            .wrap_err("Failed to create a FUSE session")?;

        if opts.gid != calling_gid {
            nix::unistd::setgid(opts.gid)
                .into_diagnostic()
                .wrap_err("Failed to set desired group (actual)")?;
            nix::unistd::setegid(opts.gid)
                .into_diagnostic()
                .wrap_err("Failed to set desired group (effective)")?;
        }
        if opts.uid != calling_uid {
            nix::unistd::setuid(opts.uid)
                .into_diagnostic()
                .wrap_err("Failed to become desired user (actual)")?;
            nix::unistd::seteuid(opts.uid)
                .into_diagnostic()
                .wrap_err("Failed to become desired user (effective)")?;
        }

        if !self.foreground {
            tracing::debug!("Moving into background...");
            // We cannot daemonize until the session is established above,
            // otherwise initial use of the filesystem may not show any mount
            // at all.
            nix::unistd::daemon(false, self.log_foreground)
                .into_diagnostic()
                .wrap_err("Failed to daemonize")?;
        }

        // We also cannot go multi-thread until the daemonization process above
        // is complete, otherwise we can end up with deadlocks. Because
        // the session needs to be established first, and this after, we
        // cannot know if the full configuration of the filesystem is correct,
        // and there may be errors which only appear at runtime.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.fuse.worker_threads.get())
            .max_blocking_threads(config.fuse.max_blocking_threads.get())
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish runtime")?;

        let result = rt.block_on(async move {
            let mut interrupt = signal(SignalKind::interrupt()).into_diagnostic().wrap_err("interrupt signal handler")?;
            let mut quit = signal(SignalKind::quit()).into_diagnostic().wrap_err("quit signal handler")?;
            let mut terminate = signal(SignalKind::terminate()).into_diagnostic().wrap_err("terminate signal handler")?;
            let (heartbeat_send, mut heartbeat_recv) = tokio::sync::mpsc::channel(1);

            tracing::info!("Starting FUSE filesystem");
            // Although the filesystem could run in the current thread, we prefer to
            // create a blocking future that can move into tokio and be managed/scheduled
            // as desired, otherwise this thread will block and may affect the runtime
            // operation unpredictably
            let unmount_callable = Arc::new(std::sync::Mutex::new(fuser_session.unmount_callable()));
            let mut join_handle = tokio::task::spawn_blocking(move || fuser_session.run());

            let mut heartbeat_monitor = config.fuse.enable_heartbeat.then(|| {
                let heartbeat_interval_seconds = config.fuse.heartbeat_interval_seconds.get();
                let heartbeat_grace_period_seconds = config.fuse.heartbeat_grace_period_seconds.get();
                // Don't move the [only] sender or if heartbeats are not
                // enabled it will be dropped and trigger the receiving end.
                let heartbeat_send = heartbeat_send.clone();
                tokio::task::spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30.min((heartbeat_interval_seconds / 2).max(1))));

                    loop {
                        interval.tick().await;
                        let seconds = session.seconds_since_last_heartbeat();
                        tracing::trace!(seconds_since_last_heartbeat = ?seconds, "heartbeat monitor");
                        if seconds > heartbeat_grace_period_seconds {
                            warn_and_sentry_event!("loss of heartbeat, shutting down filesystem");

                            // XXX: Calling unmount here has no apparent effect!
                            heartbeat_send.send(()).await.into_diagnostic().wrap_err("Failed to send unmount signal")?;
                            break;
                        }
                    }

                    Ok::<_, miette::Report>(())
                })
            });

            let mut heartbeat_failed = false;
            let res = tokio::select!{
                res = &mut join_handle => {
                    tracing::info!("Filesystem shutting down");
                    res.into_diagnostic().wrap_err("FUSE session failed")
                }
                // we explicitly catch any signal related to interruption
                // and will act by shutting down the filesystem early
                _ = terminate.recv() => Err(miette!("Terminate signal received, filesystem shutting down")),
                _ = interrupt.recv() => Err(miette!("Interrupt signal received, filesystem shutting down")),
                _ = quit.recv() => Err(miette!("Quit signal received, filesystem shutting down")),
                _ = heartbeat_recv.recv() => {
                    heartbeat_failed = true;
                    Err(miette!("Heartbeat monitor triggered, filesystem shutting down"))
                }
            };

            if let Some(handle) = heartbeat_monitor.take() {
                handle.abort_handle().abort();
                if let Err(err) = handle.await {
                    tracing::warn!("Heartbeat monitor failed: {err:?}");
                }
            }

            // The filesystem task must be fully terminated in order for the subsequent unmount
            // process to function. Otherwise, the background task will keep this process alive
            // forever.
            //
            // Exception: if spfs-monitor died without cleaning up the runtime,
            // fuser::Session::run will not exit and there's no way to tell it
            // to break out of its loop. When the fuse filesystem is included
            // in the lowerdir of an overlayfs, even though `fusermount -u`
            // appears to succeed, and the mount disappears from /etc/mtab,
            // spfs-fuse stays alive and still functions when accessed via the
            // overlayfs. As a last-ditch effort to avoid leaving a spfs-fuse
            // process running forever, we will return without attempting any
            // cleanup.
            if heartbeat_failed {
                return res;
            }
            if !join_handle.is_finished() {
                // XXX: Calling unmount has no apparent effect!
                unmount_callable.lock().unwrap().unmount().into_diagnostic().wrap_err("FUSE unmount failed")?;
                tracing::trace!("Joining FUSE session");
                // Since the umount above may have no effect, this join uses a
                // timeout so the process doesn't deadlock. Once this process
                // exits, the fusermount auto_unmount should kick in.
                match timeout(Duration::from_secs(5), join_handle).await {
                    Ok(r) => {
                        tracing::trace!("FUSE session joined");
                        r.into_diagnostic().wrap_err("FUSE join_handle await failed")?.into_diagnostic().wrap_err("FUSE session failed after unmount")?;
                    }
                    Err(_) => {
                        tracing::warn!("FUSE session join timed out");
                    }
                }
            }
            res
        });

        // we generally expect at this point that the command is complete
        // and nothing else should be executing, but it's possible that
        // we've launched long running tasks that are waiting for signals or
        // events which will never come and so we don't want to block forever
        // when the runtime is dropped.
        rt.shutdown_timeout(std::time::Duration::from_secs(2));
        result?.into_diagnostic()?;
        Ok(0)
    }

    #[cfg(windows)]
    pub fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        eprintln!("spfs-fuse is not supported on Windows.");
        Ok(1)
    }
}

#[cfg(unix)]
/// Copies from the private [`fuser::MountOption::from_str`]
fn parse_options_from_args(args: &[String]) -> Vec<MountOption> {
    args.iter()
        .map(|s| match s.as_str() {
            "auto_unmount" => MountOption::AutoUnmount,
            "allow_other" => MountOption::AllowOther,
            "allow_root" => MountOption::AllowRoot,
            "default_permissions" => MountOption::DefaultPermissions,
            "dev" => MountOption::Dev,
            "nodev" => MountOption::NoDev,
            "suid" => MountOption::Suid,
            "nosuid" => MountOption::NoSuid,
            "ro" => MountOption::RO,
            "rw" => MountOption::RW,
            "exec" => MountOption::Exec,
            "noexec" => MountOption::NoExec,
            "atime" => MountOption::Atime,
            "noatime" => MountOption::NoAtime,
            "dirsync" => MountOption::DirSync,
            "sync" => MountOption::Sync,
            "async" => MountOption::Async,
            x if x.starts_with("fsname=") => MountOption::FSName(x[7..].into()),
            x if x.starts_with("subtype=") => MountOption::Subtype(x[8..].into()),
            x => MountOption::CUSTOM(x.into()),
        })
        .collect()
}

#[cfg(unix)]
/// Checks if fusermount3 is available to be used on this system
fn fuse3_available() -> bool {
    spfs::which("fusermount3").is_some()
}
