// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spk_cli_common::flags::{self, PackageSpecifier};
use spk_cli_common::{CommandArgs, Run};

#[cfg(test)]
#[path = "./cmd_build_test/mod.rs"]
mod cmd_build_test;

/// Build a binary package from a spec file or source package.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["make", "mk"])]
pub struct Build {
    #[clap(flatten)]
    runtime: flags::Runtime,
    #[clap(flatten)]
    solver: flags::Solver,
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

    #[clap(flatten)]
    packages: flags::Packages,

    /// Build only the specified variants
    #[clap(flatten)]
    variant: flags::Variant,

    /// Allow dependencies of the package being built to have a dependency on
    /// this package.
    #[clap(long)]
    pub allow_circular_dependencies: bool,
}

#[derive(Debug)]
pub struct BuildResult {
    pub exit_status: i32,
    pub created_builds: spk_cli_common::BuildResult,
}

impl From<BuildResult> for i32 {
    fn from(result: BuildResult) -> Self {
        result.exit_status
    }
}

/// Runs make-source and then make-binary
#[async_trait::async_trait]
impl Run for Build {
    type Output = BuildResult;

    async fn run(&mut self) -> Result<Self::Output> {
        self.runtime
            .ensure_active_runtime(&["build", "make", "mk"])
            .await?;

        // divide our packages into one for each iteration of mks/mkb
        let mut runs: Vec<_> = self.packages.split();
        if runs.is_empty() {
            runs.push(Default::default());
        }

        let mut builds_for_summary = spk_cli_common::BuildResult::default();
        for mut packages in runs {
            let mut make_source = spk_cmd_make_source::cmd_make_source::MakeSource {
                options: self.options.clone(),
                verbose: self.verbose,
                packages: packages.clone(),
                runtime: self.runtime.clone(),
                created_src: spk_cli_common::BuildResult::default(),
            };
            let idents = make_source.make_source().await?;
            builds_for_summary.extend(make_source.created_src);

            // add the source ident specifier from the source build to ensure that
            // the binary build operates over this exact source package
            packages.packages = packages
                .packages
                .into_iter()
                .zip(idents.into_iter())
                .map(|(package, ident)| {
                    PackageSpecifier::WithSourceIdent((package.into_specifier(), ident.into()))
                })
                .collect();

            let mut make_binary = spk_cmd_make_binary::cmd_make_binary::MakeBinary {
                verbose: self.verbose,
                runtime: self.runtime.clone(),
                options: self.options.clone(),
                solver: self.solver.clone(),
                here: self.here,
                interactive: self.interactive,
                env: self.env,
                packages,
                variant: self.variant.clone(),
                allow_circular_dependencies: self.allow_circular_dependencies,
                created_builds: spk_cli_common::BuildResult::default(),
            };
            let exit_status = make_binary.run().await?;
            builds_for_summary.extend(make_binary.created_builds);
            if exit_status != 0 {
                return Ok(BuildResult {
                    exit_status,
                    created_builds: builds_for_summary,
                });
            }
        }

        println!("Completed builds:");
        for (_, artifact) in builds_for_summary.iter() {
            println!("   {artifact}");
        }

        Ok(BuildResult {
            exit_status: 0,
            created_builds: builds_for_summary,
        })
    }
}

impl CommandArgs for Build {
    // The important positional args for a build are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages.get_positional_args()
    }
}
