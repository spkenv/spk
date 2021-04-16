// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::*;
use structopt::StructOpt;

use spfs::{self};

#[derive(Debug, StructOpt)]
pub struct CmdLog {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Show layers from remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(about = "The tag to show history of")]
    tag: String,
}

impl CmdLog {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        let tag = spfs::tracking::TagSpec::parse(&self.tag)?;
        let tag_stream = repo.read_tag(&tag)?;
        for (i, tag) in tag_stream.into_iter().enumerate() {
            let spec = spfs::tracking::build_tag_spec(tag.org(), tag.name(), i as u64)?;
            let spec_str = spec.to_string();
            println!(
                "{} {} {} {}",
                tag.target.to_string()[..10].yellow(),
                spec_str.bold(),
                tag.user.bright_blue(),
                tag.time.to_string().green(),
            );
        }
        Ok(0)
    }
}
