// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::v0::Spec;
use spk_schema::Lint::Key;
use spk_schema::{AnyIdent, LintedItem, SpecTemplate, Template, TemplateExt};

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
    async fn run(&mut self) -> Result<i32> {
        let mut out = 0;
        let options = self.options.get_options()?;
        for spec in self.packages.iter() {
            let yaml = SpecTemplate::from_file(spec).and_then(|t| t.render_to_string(&options))?;
            let lints: std::result::Result<LintedItem<Spec<AnyIdent>>, serde_yaml::Error> =
                serde_yaml::from_str(&yaml);

            match lints {
                Ok(s) => match s.lints.is_empty() {
                    true => println!("{} {}", "OK".green(), spec.display()),
                    false => {
                        println!("{} {}:", "Failed".red(), spec.display());
                        for lint in s.lints {
                            match lint {
                                Key(k) => println!("{} {}", "----->".red(), k.generate_message()),
                            }
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
