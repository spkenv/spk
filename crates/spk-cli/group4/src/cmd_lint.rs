// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::v0::Spec;
use spk_schema::{AnyIdent, Error, LintedItem};

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
            let file_path = spec
                .canonicalize()
                .map_err(|err| Error::InvalidPath(spec.to_owned(), err))?;
            let file = std::fs::File::open(&file_path)
                .map_err(|err| Error::FileOpenError(file_path.to_owned(), err))?;
            let rdr = std::io::BufReader::new(file);

            let result: std::result::Result<LintedItem<Spec<AnyIdent>>, serde_yaml::Error> =
                serde_yaml::from_reader(rdr);

            match result {
                Ok(s) => match s.lints.is_empty() {
                    true => println!("{} {}", "OK".green(), spec.display()),
                    false => {
                        println!("{} {}:", "Failed".red(), spec.display());
                        for lint in s.lints {
                            println!("{} {}", "----->".red(), lint.message());
                        }
                        out = 1;
                    }
                },
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
