// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

use spfs;

#[macro_use]
mod args;
mod cmd_run;

main!(CmdShell);

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs-shell",
    about = "Enter a subshell in a configured spfs environment"
)]
pub struct CmdShell {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        short = "p",
        long = "pull",
        about = "try to pull the latest iteration of each tag even if it exists locally"
    )]
    pull: bool,
    #[structopt(
        short = "e",
        long = "edit",
        about = "mount the /spfs filesystem in edit mode (true if REF is empty or not given)"
    )]
    edit: bool,
    #[structopt(
        short = "n",
        long = "name",
        about = "provide a name for this runtime to make it easier to identify"
    )]
    name: Option<String>,
    #[structopt(
        name = "REF",
        about = "The tag or id of the desired runtime, use '-' or nothing to request an empty environment"
    )]
    reference: Option<String>,
}

impl CmdShell {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut run_cmd = cmd_run::CmdRun {
            verbose: self.verbose,
            pull: self.pull,
            edit: self.edit,
            name: self.name.clone(),
            reference: self.reference.clone().unwrap_or_else(|| "".into()),
            cmd: Default::default(),
            args: Default::default(),
        };
        run_cmd.run(config)
    }
}
