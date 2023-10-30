/// Generate shell completions for "spk"
use std::io::Write;

use anyhow::Result;
use clap::{value_parser, Command, Parser};
use clap_complete;
use clap_complete::Shell;
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
    pub fn run(&self, mut cmd: Command) -> Result<i32> {
        let mut buf = vec![];
        clap_complete::generate(self.shell, &mut cmd, "spk", &mut buf);
        std::io::stdout().write_all(&buf).unwrap_or(());

        Ok(0)
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
