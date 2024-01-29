// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Generate shell completions for "spk"

use std::io::Write;
use std::process::ExitStatus;

use clap::{value_parser, Command, Parser};
use clap_complete;
use clap_complete::Shell;
use miette::Result;
use spk_cli_common::CommandArgs;

/// Generate shell completions for "spk"
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct Completion {
    /// Shell syntax to emit
    #[arg(default_value_t = Shell::Bash, value_parser = value_parser!(Shell))]
    pub shell: Shell,
}

impl Completion {
    pub fn run(&self, mut cmd: Command) -> Result<ExitStatus> {
        let mut buf = vec![];
        clap_complete::generate(self.shell, &mut cmd, "spk", &mut buf);
        std::io::stdout().write_all(&buf).unwrap_or(());

        Ok(ExitStatus::default())
    }
}

impl CommandArgs for Completion {
    fn get_positional_args(&self) -> Vec<String> {
        let args: Vec<String> = vec![match self.shell {
            Shell::Bash => "bash".to_string(),
            Shell::Fish => "fish".to_string(),
            Shell::Zsh => "zsh".to_string(),
            _ => todo!(), // Shell is non-exhaustive.
        }];

        args
    }
}
