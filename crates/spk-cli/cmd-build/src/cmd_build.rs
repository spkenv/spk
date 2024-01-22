// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;

use clap::Args;
use miette::Result;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_cmd_make_binary::cmd_make_binary::PackageSpecifier;
use spk_schema::Lint::Key;

#[cfg(test)]
#[path = "./cmd_build_test.rs"]
mod cmd_build_test;

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

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

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
    #[clap(long, hide = true)]
    variant: Option<usize>,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// Allow dependencies of the package being built to have a dependency on
    /// this package.
    #[clap(long)]
    pub allow_circular_dependencies: bool,
}

/// Runs make-source and then make-binary
#[async_trait::async_trait]
impl Run for Build {
    async fn run(&mut self) -> Result<i32> {
        self.runtime
            .ensure_active_runtime(&["build", "make", "mk"])
            .await?;

        // divide our packages into one for each iteration of mks/mkb
        let mut runs: Vec<_> = self.packages.iter().map(|f| vec![f.to_owned()]).collect();
        if runs.is_empty() {
            runs.push(Vec::new());
        }

        let mut generated_lints = BTreeMap::new();
        let mut builds_for_summary = Vec::new();
        for packages in runs {
            let mut make_source = spk_cmd_make_source::cmd_make_source::MakeSource {
                options: self.options.clone(),
                verbose: self.verbose,
                packages: packages.clone(),
                runtime: self.runtime.clone(),
                created_src: std::mem::take(&mut builds_for_summary),
                lints: BTreeMap::new(),
            };
            let idents = make_source.make_source().await?;
            builds_for_summary = std::mem::take(&mut make_source.created_src);

            generated_lints.extend(std::mem::take(&mut make_source.lints));

            let mut make_binary = spk_cmd_make_binary::cmd_make_binary::MakeBinary {
                verbose: self.verbose,
                runtime: self.runtime.clone(),
                repos: self.repos.clone(),
                options: self.options.clone(),
                here: self.here,
                interactive: self.interactive,
                env: self.env,
                packages: packages
                    .into_iter()
                    .zip(idents.into_iter())
                    .map(|(package, ident)| {
                        PackageSpecifier::WithSourceIdent((package, ident.into()))
                    })
                    .collect(),
                variant: self.variant,
                formatter_settings: self.formatter_settings.clone(),
                allow_circular_dependencies: self.allow_circular_dependencies,
                created_builds: std::mem::take(&mut builds_for_summary),
            };
            let code = make_binary.run().await?;
            if code != 0 {
                return Ok(code);
            }

            builds_for_summary = std::mem::take(&mut make_binary.created_builds);
        }

        println!("Completed builds:");
        for msg in builds_for_summary.iter() {
            println!("{msg}");
        }

        if !generated_lints.is_empty() {
            println!("Lints:");
            for (pkg, lints) in generated_lints.iter() {
                for lint in lints.iter() {
                    match lint {
                        Key(k) => tracing::warn!("{} {}", pkg, k.generate_message()),
                    }
                }
            }
        }

        Ok(0)
    }
}

impl CommandArgs for Build {
    // The important positional args for a build are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages.clone()
    }
}
