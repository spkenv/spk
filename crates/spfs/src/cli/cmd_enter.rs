// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use clap::Parser;

#[macro_use]
mod args;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
main!(CmdEnter, sentry = false, sync = true);

/// Run a command in a configured spfs runtime
#[derive(Debug, Parser)]
#[clap(name = "spfs-enter")]
pub struct CmdEnter {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Remount the overlay filesystem, don't enter a new namepace
    #[clap(short, long)]
    remount: bool,

    /// The root directory of the spfs runtime being entered
    runtime_root: String,

    cmd: Option<OsString>,
    args: Vec<OsString>,
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                spfs::Error::String(format!("Failed to establish async runtime: {:?}", err))
            })?;
        rt.block_on(self.run_async(config))
    }

    pub async fn run_async(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime = spfs::runtime::Runtime::new(&self.runtime_root)?;
        if self.remount {
            spfs::reinitialize_runtime(&runtime).await?;
            Ok(0)
        } else {
            let cmd = match self.cmd.take() {
                Some(cmd) => cmd,
                None => return Err("command is required and was not given".into()),
            };

            tracing::debug!("initalizing runtime");
            spfs::initialize_runtime(&runtime, config).await?;
            runtime.ensure_startup_scripts()?;

            tracing::trace!("{:?} {:?}", cmd, self.args);
            use std::os::unix::ffi::OsStrExt;
            let cmd = std::ffi::CString::new(cmd.as_bytes()).unwrap();
            let mut args: Vec<_> = self
                .args
                .iter()
                .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
                .collect();
            args.insert(0, cmd.clone());
            nix::unistd::execv(cmd.as_ref(), args.as_slice())?;
            Ok(0)
        }
    }
}
