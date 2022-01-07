// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use structopt::StructOpt;

#[macro_use]
mod args;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
main!(CmdEnter, sentry = false, sync = true);

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs-enter",
    about = "Run a command in a configured spfs runtime"
)]
pub struct CmdEnter {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,

    #[structopt(
        short = "r",
        long = "remount",
        about = "remount the overlay filesystem, don't enter a new namepace"
    )]
    remount: bool,

    #[structopt()]
    runtime_root: String,

    #[structopt()]
    cmd: Option<OsString>,
    #[structopt()]
    args: Vec<OsString>,
}

impl CmdEnter {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        // Acquire expected effective caps.

        use caps::{CapSet, Capability};

        let mut current_caps = caps::read(None, CapSet::Effective)?;
        for needed_cap in &[
            // These were formerly already effective by default
            // via `setcap`, before the addition of CAP_FOWNER,
            // which we do not want to be effective by default.
            // It is not legal to set some caps with `+ep` and
            // others with just `+p`.
            Capability::CAP_SETUID,
            Capability::CAP_CHOWN,
            Capability::CAP_MKNOD,
            Capability::CAP_SYS_ADMIN,
        ] {
            current_caps.insert(*needed_cap);
        }
        caps::set(None, CapSet::Effective, &current_caps)?;

        let runtime = spfs::runtime::Runtime::new(&self.runtime_root)?;
        if self.remount {
            spfs::reinitialize_runtime(&runtime)?;
            Ok(0)
        } else {
            let cmd = match self.cmd.take() {
                Some(cmd) => cmd,
                None => return Err("command is required and was not given".into()),
            };
            spfs::initialize_runtime(&runtime, config)?;

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
