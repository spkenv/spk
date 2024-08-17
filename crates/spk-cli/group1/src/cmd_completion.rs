// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Generate shell completions for "spk"

use std::io::Write;

use clap::{Command, Parser, value_parser};
use clap_complete;
use clap_complete::{Generator, Shell};
use clap_complete_nushell::Nushell;
use clap::ValueEnum;
use miette::Result;
use spk_cli_common::CommandArgs;

#[derive(Clone, Debug, ValueEnum)]
enum ShellCompletion {
    /// Bourne Again SHell (bash)
    Bash,
    /// Friendly Interactive SHell (fish)
    Fish,
    /// PowerShell
    Zsh,
    /// Nushell
    Nushell,
}
impl ToString for ShellCompletion {
    fn to_string(&self) -> String {
        match self {
            ShellCompletion::Bash => "bash".to_string(),
            ShellCompletion::Fish => "fish".to_string(),
            ShellCompletion::Zsh => "zsh".to_string(),
            ShellCompletion::Nushell => "nu".to_string(),
        }
    }
}

impl Generator for ShellCompletion {
    /// Generate the file name for the completion script.
    fn file_name(&self, name: &str) -> String {
        match self {
            ShellCompletion::Bash => Shell::Bash.file_name(name),
            ShellCompletion::Fish => Shell::Fish.file_name(name),
            ShellCompletion::Zsh => Shell::Zsh.file_name(name),
            ShellCompletion::Nushell => Nushell.file_name(name),
        }
    }

    /// Generate the completion script for the shell.
    fn generate(&self, cmd: &clap::Command, buf: &mut dyn std::io::Write) {
        match self {
            ShellCompletion::Bash => Shell::Bash.generate(cmd, buf),
            ShellCompletion::Fish => Shell::Fish.generate(cmd, buf),
            ShellCompletion::Zsh => Shell::Zsh.generate(cmd, buf),
            ShellCompletion::Nushell => Nushell.generate(cmd, buf),
        }
    }
}

/// Generate shell completions for "spk"
#[derive(Parser, Clone, Debug)]
#[command(author, about, long_about)]
pub struct Completion {
    /// Shell syntax to emit
    #[arg(default_value_t = ShellCompletion::Bash, value_parser = value_parser!(ShellCompletion))]
    shell: ShellCompletion,
}

impl Completion {
    pub fn run(&self, mut cmd: Command) -> Result<i32> {
        let mut buf = vec![];
        clap_complete::generate(self.shell.clone(), &mut cmd, "spk", &mut buf);
        std::io::stdout().write_all(&buf).unwrap_or(());
        Ok(0)
    }
}

impl CommandArgs for Completion {
    fn get_positional_args(&self) -> Vec<String> {
        let args: Vec<String> = vec![match self.shell {
            ShellCompletion::Bash => "bash".to_string(),
            ShellCompletion::Fish => "fish".to_string(),
            ShellCompletion::Zsh => "zsh".to_string(),
            ShellCompletion::Nushell => "nu".to_string(),
        }];

        args
    }
}
