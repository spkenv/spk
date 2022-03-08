// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdRuntimes {
    #[structopt(
        short = "q",
        long = "quiet",
        about = "Only print the name of each runtime, no additional data"
    )]
    quiet: bool,
}

impl CmdRuntimes {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = config.get_runtime_storage()?;
        for runtime in runtime_storage.iter_runtimes() {
            let runtime = runtime?;
            let mut message = runtime.reference().to_string_lossy().to_string();
            if !self.quiet {
                message = format!(
                    "{message}\trunning={}\tpid={:?}\teditable={}",
                    runtime.is_running(),
                    runtime.get_pid(),
                    runtime.is_editable()
                )
            }
            println!("{message}");
        }
        Ok(0)
    }
}
