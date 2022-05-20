// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::{Args, Subcommand};
use futures::TryStreamExt;

/// View and manage spfs runtime information
#[derive(Debug, Args)]
#[clap(visible_alias = "rt")]
pub struct CmdRuntime {
    #[clap(subcommand)]
    command: Command,
}

impl CmdRuntime {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        self.command.run(config).await
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Info(CmdInfo),
    List(CmdList),
    Remove(CmdRemove),
}

impl Command {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        match self {
            Self::Info(cmd) => cmd.run(config).await,
            Self::List(cmd) => cmd.run(config).await,
            Self::Remove(cmd) => cmd.run(config).await,
        }
    }
}

/// List runtime information from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "ls")]
pub struct CmdList {
    /// List runtimes in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Only print the name of each runtime, no additional data
    #[clap(short, long)]
    quiet: bool,
}

impl CmdList {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.try_next().await? {
            let mut message = runtime.name().to_string();
            if !self.quiet {
                message = format!(
                    "{message}\trunning={}\tpid={:?}\teditable={}",
                    runtime.status.running, runtime.status.owner, runtime.status.editable
                )
            }
            println!("{message}");
        }
        Ok(0)
    }
}

/// Show the complete state of a runtime
#[derive(Debug, Args)]
pub struct CmdInfo {
    /// Load a runtime in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// The name/id of the runtime to remove
    #[clap(env = "SPFS_RUNTIME")]
    name: String,
}

impl CmdInfo {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let runtime = runtime_storage.read_runtime(&self.name).await?;
        serde_json::to_writer_pretty(std::io::stdout(), runtime.data())?;
        println!(); // the trailing new line is nice for interactive shells

        Ok(0)
    }
}

/// List runtime information from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "rm")]
pub struct CmdRemove {
    /// Remove a runtime in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Remove the runtime from the repository forcefully
    ///
    /// Even if the monitor cannot be stopped or killed the
    /// data will be removed from the repository.
    #[clap(short, long)]
    force: bool,

    /// Remove the runtime even if it's owned by someone else
    #[clap(long)]
    ignore_user: bool,

    /// Remove the runtime even if it appears to be from a different host
    ///
    /// Implies --ignore-monitor
    #[clap(long)]
    ignore_host: bool,

    /// Do not try and terminate the monitor process, just remove runtime data
    #[clap(long)]
    ignore_monitor: bool,

    /// The name/id of the runtime to remove
    name: String,
}

impl CmdRemove {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let runtime = runtime_storage.read_runtime(&self.name).await?;

        let default_author = spfs::runtime::Author::default();
        let is_same_author = runtime.author.user_name == default_author.user_name;
        if !self.ignore_user && !is_same_author {
            tracing::error!(
                "Won't delete, this runtime belongs to '{}'",
                runtime.author.user_name
            );
            tracing::error!(" > use --ignore-user to ignore this error");
            return Ok(1);
        }

        let is_same_host = runtime.author.host_name == default_author.host_name;
        if !self.ignore_host && !is_same_host {
            tracing::error!(
                "Won't delete, this runtime was spawned on a different machine: '{}'",
                runtime.author.host_name
            );
            tracing::error!(" > use --ignore-host to ignore this error");
            return Ok(1);
        }

        if !self.ignore_monitor && is_same_host && is_monitor_running(&runtime) {
            tracing::error!("Won't delete, the monitor process appears to still be running",);
            tracing::error!(" > terminating the command should trigger the cleanup process");
            tracing::error!(" > use --ignore-monitor to ignore this error");
            return Ok(1);
        }

        runtime_storage.remove_runtime(runtime.name()).await?;

        Ok(0)
    }
}

fn is_monitor_running(rt: &spfs::runtime::Runtime) -> bool {
    if let Some(pid) = rt.status.monitor {
        // we are blatantly ignoring the fact that this pid might
        // have been reused and is not the monitor anymore. Given
        // that there will always be a race condition to this effect
        // even if we did try to check the command line args for this
        // process. So we stick on the extra conservative side
        is_process_running(pid)
    } else {
        false
    }
}

fn is_process_running(pid: u32) -> bool {
    // sending a null signall to the pid just allows us to check
    // if the process actually exists without affecting it
    let pid = nix::unistd::Pid::from_raw(pid as i32);
    nix::sys::signal::kill(pid, None).is_ok()
}
