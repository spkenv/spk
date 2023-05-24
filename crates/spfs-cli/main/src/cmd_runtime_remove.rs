// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

/// Remove runtimes from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "rm")]
pub struct CmdRuntimeRemove {
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

    /// Allow durable runtimes to be removed, normally they will not be removed
    #[clap(long)]
    remove_durable: bool,

    /// The name/id of the runtime to remove
    name: Vec<String>,
}

impl CmdRuntimeRemove {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        for runtime_name in self.name.iter() {
            let runtime = runtime_storage.read_runtime(&runtime_name).await?;

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

            if runtime.keep_runtime() && !self.remove_durable {
                tracing::error!("Won't delete, the runtime is marked as durable and");
                tracing::error!(" > '--remove-durable' was not specified");
                tracing::error!(" > use --remove-durable to remove a durable runtime");
                return Ok(2);
            }

            runtime_storage.remove_runtime(runtime.name()).await?;
        }

        Ok(0)
    }
}

pub(crate) fn is_monitor_running(rt: &spfs::runtime::Runtime) -> bool {
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

#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    // sending a null signal to the pid just allows us to check
    // if the process actually exists without affecting it
    let pid = nix::unistd::Pid::from_raw(pid as i32);
    nix::sys::signal::kill(pid, None).is_ok()
}

#[cfg(windows)]
fn is_process_running(pid: u32) -> bool {
    // PROCESS_SYNCHRONIZE seems like the most limited access we can request,
    // which simply allows us to wait on the PID
    let access = windows::Win32::System::Threading::PROCESS_SYNCHRONIZE;
    let result = unsafe { windows::Win32::System::Threading::OpenProcess(access, false, pid) };
    let Ok(handle) = result else {
        return false;
    };
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
    true
}
