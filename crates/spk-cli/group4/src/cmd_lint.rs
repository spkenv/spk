// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::{SpecTemplate, Template, TemplateExt};

/// Validate spk yaml files
#[derive(Args)]
pub struct Lint {
    #[clap(flatten)]
    options: flags::Options,

    /// Yaml file(s) to validate
    packages: Vec<PathBuf>,
}

#[async_trait::async_trait]
impl Run for Lint {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;
        let mut out = 0;
        for spec in self.packages.iter() {
            let result = SpecTemplate::from_file(spec).and_then(|t| t.render(&options));
            match result {
                Ok(_) => println!("{} {}", "OK".green(), spec.display()),
                Err(err) => {
                    println!(
                        "{} {}:\n{} {err}",
                        "Failed".red(),
                        spec.display(),
                        "----->".red()
                    );
                    out = 1;
                }
            }
        }
        Ok(out)
    }
}

impl CommandArgs for Lint {
    fn get_positional_args(&self) -> Vec<String> {
        self.packages
            .iter()
            .map(PathBuf::as_path)
            .map(Path::to_string_lossy)
            .map(|p| p.to_string())
            .collect()
    }
}
