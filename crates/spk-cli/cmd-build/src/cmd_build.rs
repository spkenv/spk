// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spk_cli_common::flags::{self, PackageSpecifier};
use spk_cli_common::{CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::{BuildIdent, VersionIdent};
use spk_storage;

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

        // Get the local repo, where the builds currently are, for the
        // disk usage calculations.
        let local_repo = spk_storage::local_repository().await?;
        let repos: Vec<(String, spk_storage::RepositoryHandle)> =
            vec![("local".into(), local_repo.into())];

        let mut idents: Vec<BuildIdent> = Vec::new();
        let mut current_package = None;
        let mut number_of_packages = 0;
        // Total disk usage of all the builds on their own (includes
        // double counting within versions)
        let mut total_builds_size = 0;
        // Total disk usage of all package/versions' disk usage (not
        // double counted within versions or builds)
        let mut total_packages_size = 0;

        println!("Completed builds:");
        for (_, artifact) in builds_for_summary.iter() {
            let ident = artifact.build_ident();
            if current_package.is_none() {
                current_package = Some(ident.clone().to_version_ident());
                number_of_packages += 1;
            }

            let package_version = ident.clone().to_version_ident();
            if let Some(ref current_package_version) = current_package {
                if package_version == *current_package_version {
                    idents.push(ident.clone());
                } else {
                    // Package has changed, show the total disk usage
                    // for the current package's builds that were just
                    // built. This will not double count shared
                    // objects between those builds.
                    let version_builds_size = spk_storage::get_version_builds_disk_usage(
                        &repos,
                        current_package_version,
                        &idents,
                    )
                    .await
                    .unwrap_or(0);

                    total_packages_size += version_builds_size;
                    self.print_total_size(current_package_version, version_builds_size)
                        .await;

                    // Update the current package to the next one and
                    // start collecting its builds.
                    current_package = Some(package_version);
                    number_of_packages += 1;
                    idents.clear();
                    idents.push(ident.clone());
                }
            }

            // Build sizes are calculated separately because we want
            // to show the disk usage of each build on its own.
            let size = spk_storage::get_build_disk_usage(&repos, ident)
                .await
                .unwrap_or(0);

            // This total will include some double counting in some packages
            total_builds_size += size;
            println!("   {artifact}  [{}]", spk_storage::human_readable(size));
        }

        // Show the total disk usage for the last package's build. This
        // will not double count shared things between those builds.
        if let Some(ref current_package_version) = current_package {
            let version_builds_size = spk_storage::get_version_builds_disk_usage(
                &repos,
                current_package_version,
                &idents,
            )
            .await
            .unwrap_or(0);

            total_packages_size += version_builds_size;
            self.print_total_size(current_package_version, version_builds_size)
                .await;
        }

        // Only show the total of the builds, the double counting one,
        // when the user wants more info for comparisons or debugging.
        if self.verbose > 0 {
            println!(
                "Total disk usage of all builds:  {}",
                spk_storage::human_readable(total_builds_size)
            );
        }

        // Output the total of all the packages' sizes when multiple
        // package/versions were built.
        if number_of_packages > 1 {
            println!(
                "Total disk usage for all packages built: {}",
                spk_storage::human_readable(total_packages_size),
            );
        }

        Ok(BuildResult {
            exit_status: 0,
            created_builds: builds_for_summary,
        })
    }
}

impl Build {
    async fn print_total_size(&self, package_version: &VersionIdent, size: u64) {
        println!(
            "Total disk usage for these {} builds:  {}",
            package_version.format_ident(),
            spk_storage::human_readable(size)
        );
    }
}

impl CommandArgs for Build {
    // The important positional args for a build are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages.get_positional_args()
    }
}
