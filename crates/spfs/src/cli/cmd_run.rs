// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;

use clap::Parser;

use spfs::prelude::*;

/// Run a program in a configured spfs environment
#[derive(Debug, Parser)]
#[clap(name = "spfs-run")]
pub struct CmdRun {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Try to pull the latest iteration of each tag even if it exists locally
    #[clap(short, long)]
    pub pull: bool,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    pub edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    pub name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or an empty string to request an empty environment
    pub reference: String,

    pub cmd: OsString,
    pub args: Vec<OsString>,
}

impl CmdRun {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_local_repository().await?;
        let runtimes = config.get_runtime_storage()?;
        let mut runtime = match &self.name {
            Some(name) => runtimes.create_named_runtime(name)?,
            None => runtimes.create_runtime()?,
        };
        match self.reference.as_str() {
            "-" | "" => self.edit = true,
            reference => {
                let env_spec = spfs::tracking::parse_env_spec(reference)?;
                for target in env_spec {
                    let target = target.to_string();
                    if self.pull || !repo.has_ref(target.as_str()).await {
                        tracing::info!(reference = ?target, "pulling target ref");
                        spfs::pull_ref(target.as_str()).await?
                    }

                    let obj = repo.read_ref(target.as_str()).await?;
                    runtime.push_digest(&obj.digest()?)?;
                }
            }
        }

        runtime.set_editable(self.edit)?;
        tracing::debug!("resolving entry process");
        let (cmd, args) =
            spfs::build_command_for_runtime(&runtime, self.cmd.clone(), &mut self.args)?;
        tracing::trace!("{:?} {:?}", cmd, args);
        use std::os::unix::ffi::OsStrExt;
        let cmd = std::ffi::CString::new(cmd.as_bytes()).unwrap();
        let mut args: Vec<_> = args
            .into_iter()
            .map(|arg| std::ffi::CString::new(arg.as_bytes()).unwrap())
            .collect();
        args.insert(0, cmd.clone());
        runtime.set_running(true)?;
        nix::unistd::execv(cmd.as_ref(), args.as_slice())?;
        Ok(0)
    }
}
