// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

#[macro_use]
mod args;
mod cmd_run;

main!(CmdShell);

/// Enter a subshell in a configured spfs environment
#[derive(Debug, Parser)]
#[clap(name = "spfs-shell")]
pub struct CmdShell {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Try to pull the latest iteration of each tag even if it exists locally
    #[clap(short, long)]
    pull: bool,

    /// Mount the spfs filesystem in edit mode (true if REF is empty or not given)
    #[clap(short, long)]
    edit: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(short, long)]
    name: Option<String>,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or nothing to request an empty environment
    #[clap(name = "REF")]
    reference: Option<String>,
}

impl CmdShell {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut run_cmd = cmd_run::CmdRun {
            verbose: self.verbose,
            pull: self.pull,
            edit: self.edit,
            name: self.name.clone(),
            reference: self.reference.clone().unwrap_or_else(|| "".into()),
            cmd: Default::default(),
            args: Default::default(),
        };
        run_cmd.run(config).await
    }
}
