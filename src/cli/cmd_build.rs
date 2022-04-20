// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::{flags, Run};

/// Build a binary package from a spec file or source package.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["make", "mk"])]
pub struct Build {
    #[clap(flatten)]
    runtime: flags::Runtime,
    #[clap(flatten)]
    repos: flags::Repositories,
    #[clap(flatten)]
    options: flags::Options,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// Build from the current directory, instead of a source package)
    #[clap(long)]
    here: bool,

    /// Setup the build, but instead of running the build script start an interactive shell
    #[clap(long, short)]
    interactive: bool,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    env: bool,

    /// The package names or yaml spec files to build
    #[clap(name = "NAME|SPEC_FILE")]
    packages: Vec<String>,

    /// Build only the specified variant, by index, if defined
    #[clap(long)]
    variant: Option<usize>,
}

/// Runs make-source and then make-binary
impl Run for Build {
    fn run(&mut self) -> Result<i32> {
        self.runtime.ensure_active_runtime()?;

        // divide our packages into one for each iteration of mks/mkb
        let mut runs: Vec<_> = self.packages.iter().map(|f| vec![f.to_owned()]).collect();
        if runs.is_empty() {
            runs.push(Vec::new());
        }

        for packages in runs {
            let mut make_source = super::cmd_make_source::MakeSource {
                verbose: self.verbose,
                packages: packages.clone(),
                runtime: self.runtime.clone(),
            };
            let code = make_source.run()?;
            if code != 0 {
                return Ok(code);
            }

            let mut make_binary = super::cmd_make_binary::MakeBinary {
                verbose: self.verbose,
                runtime: self.runtime.clone(),
                repos: self.repos.clone(),
                options: self.options.clone(),
                here: self.here,
                interactive: self.interactive,
                env: self.env,
                packages,
                variant: self.variant,
            };
            let code = make_binary.run()?;
            if code != 0 {
                return Ok(code);
            }
        }

        Ok(0)
    }
}
