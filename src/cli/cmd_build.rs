// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Build a binary package from a spec file or source package.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["mkbinary", "mkbin", "mkb"])]
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

    /// The packages or yaml spec files to build
    #[clap(required = true, name = "SPEC_FILE")]
    files: Vec<String>,
}

/// Runs make-source and then make-binary
impl Build {
    pub fn run(&self) -> Result<i32> {
        self.runtime.ensure_active_runtime()?;

        for filename in self.files.iter() {
            let make_source = super::cmd_make_source::MakeSource {
                verbose: self.verbose,
                runtime: self.runtime.clone(),
                packages: vec![filename.to_owned()],
            };
            let code = make_source.run()?;
            if code != 0 {
                return Ok(code);
            }

            let make_binary = super::cmd_make_binary::MakeBinary {
                verbose: self.verbose,
                runtime: self.runtime.clone(),
                repos: self.repos.clone(),
                options: self.options.clone(),
                here: self.here,
                interactive: self.interactive,
                env: self.env,
                packages: vec![filename.to_owned()],
            };
            let code = make_binary.run()?;
            if code != 0 {
                return Ok(code);
            }
        }

        Ok(0)
    }
}
