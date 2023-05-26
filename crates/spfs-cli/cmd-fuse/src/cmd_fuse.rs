// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use fuser::MountOption;
use spfs::tracking::EnvSpec;
use spfs::Error;
use spfs_cli_common as cli;
use spfs_vfs::{Config, Session};
use tokio::signal::unix::{signal, SignalKind};

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
    opt.logging.configure();

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return 1;
        }
        Ok(config) => config,
    };
    let result = opt.run(&config);

    spfs_cli_common::handle_result!(result)
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
    /// Use '-' or nothing to request an empty environment
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
    pub fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let calling_uid = nix::unistd::geteuid();
        let calling_gid = nix::unistd::getegid();

        // these will cause conflicts later on if their counterpart is also provided
        let required_opts = vec![
            MountOption::RO,
            MountOption::NoDev,
            MountOption::NoSuid,
            MountOption::CUSTOM("nonempty".into()),
        ];
        let mut opts = Config {
            root_mode: 0o777,
            uid: calling_uid,
            gid: calling_gid,
            remotes: Vec::new(),
            mount_options: required_opts.into_iter().collect(),
        };

        let parsed_opts = parse_options_from_args(&self.options);
        for option in parsed_opts {
            match option {
                MountOption::CUSTOM(opt) => {
                    match opt.split_once('=') {
                        Some(("remote", name)) => {
                            opts.remotes.push(name.to_owned());
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

        if opts.gid != calling_gid {
            nix::unistd::setgid(opts.gid).context("Failed to set desired group (actual)")?;
            nix::unistd::setegid(opts.gid).context("Failed to set desired group (effective)")?;
        }
        if opts.uid != calling_uid {
            nix::unistd::setuid(opts.uid).context("Failed to become desired user (actual)")?;
            nix::unistd::seteuid(opts.uid).context("Failed to become desired user (effective)")?;
        }

        let mountpoint = self
            .mountpoint
            .canonicalize()
            .context("Invalid mount point")?;

        if !calling_uid.is_root() {
            // unprivileged callers must have write access to the directory that
            // they are trying to mount over.
            nix::unistd::access(&mountpoint, nix::unistd::AccessFlags::W_OK)
                .context("Must have write access to mountpoint")?;
        }

        tracing::debug!("Establishing fuse session...");
        let mount_opts = opts.mount_options.iter().cloned().collect::<Vec<_>>();
        let mut session = fuser::Session::new(
            Session::new(self.reference.clone(), opts),
            &mountpoint,
            &mount_opts,
        )
        .context("Failed to create a FUSE session")?;

        if !self.foreground {
            tracing::debug!("Moving into background...");
            // We cannot daemonize until the session is established above,
            // otherwise initial use of the filesystem may not show any mount
            // at all.
            nix::unistd::daemon(false, self.log_foreground)?;
        }

        // We also cannot go multi-thread until the daemonization process above
        // is complete, otherwise we can end up with deadlocks. Because
        // the session needs to be established first, and this after, we
        // cannot know if the full configuration of the filesystem is correct,
        // and there may be errors which only appear at runtime.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("Failed to establish runtime")?;

        let result = rt.block_on(async move {
            let mut interrupt = signal(SignalKind::interrupt()).context("interrupt signal handler")?;
            let mut quit = signal(SignalKind::quit()).context("quit signal handler")?;
            let mut terminate = signal(SignalKind::terminate()).context("terminate signal handler")?;

            tracing::info!("Starting FUSE filesystem");
            // Although the filesystem could run in the current thread, we prefer to
            // create a blocking future that can move into tokio and be managed/scheduled
            // as desired, otherwise this thread will block and may affect the runtime
            // operation unpredictably
            let fut = tokio::task::spawn_blocking(move || session.run());
            tokio::select!{
                res = fut => {
                    tracing::info!("Filesystem shutting down");
                    res.context("FUSE session failed")
                }
                // we explicitly catch any signal related to interruption
                // and will act by shutting down the filesystem early
                _ = terminate.recv() => Err(anyhow!("Terminate signal received, filesystem shutting down")),
                _ = interrupt.recv() => Err(anyhow!("Interrupt signal received, filesystem shutting down")),
                _ = quit.recv() => Err(anyhow!("Quit signal received, filesystem shutting down")),
            }
        });

        // we generally expect at this point that the command is complete
        // and nothing else should be executing, but it's possible that
        // we've launched long running tasks that are waiting for signals or
        // events which will never come and so we don't want to block forever
        // when the runtime is dropped.
        rt.shutdown_timeout(std::time::Duration::from_secs(2));
        result??;
        Ok(0)
    }
}

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
