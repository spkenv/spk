// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::{parse_ident, AnyIdent};
use spk_schema::{Deprecate, DeprecateMut, Package, Recipe, Spec, SpecRecipe};
use spk_storage::{self as storage};

#[cfg(test)]
#[path = "./cmd_deprecate_test.rs"]
mod cmd_deprecate_test;

/// Deprecate or undeprecate actions
#[derive(PartialEq, Eq)]
pub(crate) enum ChangeAction {
    Deprecate,
    Undeprecate,
}

// Methods for producing text for various printouts
impl ChangeAction {
    fn as_str(&self) -> &'static str {
        match self {
            ChangeAction::Deprecate => "deprecate",
            ChangeAction::Undeprecate => "undeprecate",
        }
    }

    fn as_past_tense(&self) -> &'static str {
        match self {
            ChangeAction::Deprecate => "deprecated",
            ChangeAction::Undeprecate => "undeprecated",
        }
    }

    fn as_present_tense(&self) -> &'static str {
        match self {
            ChangeAction::Deprecate => "Deprecating",
            ChangeAction::Undeprecate => "Undeprecating",
        }
    }

    fn as_capitalized(&self) -> &'static str {
        match self {
            ChangeAction::Deprecate => "Deprecate",
            ChangeAction::Undeprecate => "Undeprecate",
        }
    }

    fn as_alternate(&self) -> &'static str {
        match self {
            ChangeAction::Deprecate => "retire",
            ChangeAction::Undeprecate => "restore",
        }
    }
}

/// Deprecate packages in a repository.
///
/// Deprecated packages can still be resolved by requesting the exact
/// build, but will otherwise not show up in environments. By
/// deprecating a package version, as opposed to an individual build,
/// the package will also no longer be rebuilt from source under any
/// circumstances. Deprecating a package version also deprecates all
/// builds by association.
#[derive(Args, Clone)]
pub struct DeprecateCmd {
    #[clap(flatten)]
    repos: flags::Repositories,

    /// If set, answer 'Yes' to all confirmation prompts
    #[clap(long, short)]
    pub yes: bool,

    /// The package version or build to deprecate
    ///
    /// By deprecating a package version, as opposed to an individual
    /// build, the package will also no longer be rebuilt from source
    /// under any circumstances. Deprecating a package version also
    /// deprecates all its builds by association.
    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,
}

/// Deprecate (hide) packages in a repository
#[async_trait::async_trait]
impl Run for DeprecateCmd {
    async fn run(&mut self) -> Result<i32> {
        change_deprecation_state(
            ChangeAction::Deprecate,
            &self.repos.get_repos_for_destructive_operation().await?,
            &self.packages,
            self.yes,
        )
        .await
    }
}

impl CommandArgs for DeprecateCmd {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a deprecate are the packages
        self.packages.clone()
    }
}

// TODO: probably should be somewhere else because other commands might
// use it in future, e.g. io.rs?
//
/// Display a question for the user and get their input, typically to
/// stdout and from stdin
pub(crate) fn ask_user(prompt: &str) -> String {
    print!("{prompt}");
    let _ = std::io::stdout().flush();

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Unable to read user's response");
    input
}

/// Changes package builds' specs' deprecation field based on the
/// given action. Deprecating sets it to true, undeprecating to false.
pub(crate) async fn change_deprecation_state(
    action: ChangeAction,
    repositories: &[(String, storage::RepositoryHandle)],
    packages: &[String],
    yes: bool,
) -> Result<i32> {
    let repos: Vec<_> = repositories
        .iter()
        .map(|(name, repo)| (name, Arc::new(repo)))
        .collect();
    if repos.is_empty() {
        eprintln!(
            "{}",
            "No repositories selected, specify --enable-repo (-r), or remove --no-local-repo"
                .yellow()
        );
        return Ok(1);
    }

    // Find and load everything that we want to action first to avoid
    // doing some actions and then failing in the middle of the
    // operation. This is still not properly atomic but avoids the
    // simple failure cases.
    let mut to_action = Vec::new();
    for name in packages.iter() {
        if !name.contains('/') {
            tracing::error!("Must provide a version number: {name}/<VERSION NUMBER>");
            tracing::error!(
                " > use 'spk ls {name}' or 'spk ls {name} -r <REPO_NAME>' to view available versions"
            );
            return Ok(2);
        }
        if name.ends_with('/') {
            tracing::error!("A trailing '/' isn't a valid version number or build digest in '{name}'. Please remove the trailing '/', or specify a version number or build digest after it.");
            return Ok(3);
        }

        let ident = parse_ident(name)?;
        for (repo_name, repo) in repos.iter() {
            match ident.clone().into_inner() {
                (ident, None) => {
                    match repo.read_recipe(&ident).await {
                        Ok(recipe) => {
                            to_action.push((
                                DeprecationTarget::Recipe(recipe),
                                repo_name,
                                Arc::clone(repo),
                            ));
                            // It's a package version, so find and add its
                            // builds from this repo
                            print!(
                            "{ident} is a package version, adding its builds from {repo_name}... "
                        );
                            let mut count = 0;
                            let builds = match repo.list_package_builds(&ident).await {
                                Ok(idents) => idents,
                                Err(err) => {
                                    tracing::debug!("No {ident} build found in {repo_name}: {err}");
                                    continue;
                                }
                            };
                            for build in builds {
                                if build.is_embedded() {
                                    // Don't attempt to deprecate an embedded package; the stub
                                    // will be deprecated when deprecating its provider.
                                    continue;
                                }
                                let spec = match repo.read_package(&build).await {
                                    Ok(b) => b,
                                    Err(err) => {
                                        tracing::debug!(
                                        "Unable to read {build} build spec from {repo_name}: {err}"
                                    );
                                        continue;
                                    }
                                };

                                to_action.push((
                                    DeprecationTarget::Package(spec),
                                    repo_name,
                                    Arc::clone(repo),
                                ));
                                count += 1;
                            }
                            println!("{count} found");
                        }
                        Err(err) => {
                            tracing::debug!(
                                "Unable to read recipe {ident} from {repo_name}: {err}"
                            );
                            continue;
                        }
                    }
                }
                (ident, Some(build)) => {
                    let ident = ident.to_build(build);
                    match repo.read_package(&ident).await {
                        Ok(package) => {
                            to_action.push((
                                DeprecationTarget::Package(package),
                                repo_name,
                                Arc::clone(repo),
                            ));
                        }
                        Err(err) => {
                            tracing::debug!(
                                "Unable to read package {ident} from {repo_name}: {err}"
                            );
                            continue;
                        }
                    }
                }
            }
        }
    }

    // Tell the user when there are no packages to action
    if to_action.is_empty() {
        println!("No packages found to {}. Nothing to do.", action.as_str());
        return Ok(4);
    }

    // Summarise what is about to be actioned. Note, this does not
    // show whether the action will change any of the items.
    let pkg_text = if to_action.len() > 1 {
        "packages"
    } else {
        "package"
    };
    println!(
        "About to {} {} {pkg_text}:",
        action.as_str(),
        to_action.len()
    );
    for (spec, repo_name, _) in to_action.iter() {
        println!("  {} (in {repo_name})", spec.ident().format_ident());
    }

    // Ask the user if they are sure they want to do the action on
    // all the builds. If the --yes option was given on the
    // command line, skip the prompt and assume they are sure.
    if !yes {
        let prompt = &format!(
            "Do you want to {} ({}) ALL these packages? [y/N]: ",
            action.as_str(),
            action.as_alternate()
        );
        let response = ask_user(prompt);
        match response.to_lowercase().trim() {
            "y" | "yes" => {}
            _ => {
                // User didn't confirm the action, so don't perform
                // any action, just exit
                println!(
                    "{} canceled. Things will remain as they were.",
                    action.as_capitalized()
                );
                return Ok(5);
            }
        }
    }

    // Change all the item's statuses to the correct state based on
    // the action, unless they are already in that state.
    let new_status = action == ChangeAction::Deprecate;
    for (mut target, repo_name, repo) in to_action.into_iter() {
        let fmt = target.ident().format_ident();

        if target.is_deprecated() == new_status {
            println!(
                " {} {fmt} in {repo_name}, it is already {}.",
                "Skipping".yellow(),
                action.as_past_tense(),
            );
            continue;
        }

        println!("{} {fmt} in {repo_name}", action.as_present_tense(),);

        match action {
            ChangeAction::Deprecate => target.deprecate()?,
            ChangeAction::Undeprecate => target.undeprecate()?,
        }
        match target {
            DeprecationTarget::Recipe(r) => repo.force_publish_recipe(&r).await?,
            DeprecationTarget::Package(p) => repo.update_package(&p).await?,
        }
        tracing::info!(repo=%repo_name, "{} {fmt}", action.as_past_tense());
    }
    Ok(0)
}
enum DeprecationTarget {
    Recipe(Arc<SpecRecipe>),
    Package(Arc<Spec>),
}

impl Deprecate for DeprecationTarget {
    fn is_deprecated(&self) -> bool {
        match self {
            DeprecationTarget::Recipe(t) => t.is_deprecated(),
            DeprecationTarget::Package(t) => t.is_deprecated(),
        }
    }
}

impl DeprecateMut for DeprecationTarget {
    fn deprecate(&mut self) -> spk_schema::Result<()> {
        match self {
            DeprecationTarget::Recipe(t) => {
                let mut new = (**t).clone();
                new.deprecate()?;
                let _ = std::mem::replace(t, new.into());
            }
            DeprecationTarget::Package(t) => {
                let mut new = (**t).clone();
                new.deprecate()?;
                let _ = std::mem::replace(t, new.into());
            }
        }
        Ok(())
    }

    fn undeprecate(&mut self) -> spk_schema::Result<()> {
        match self {
            DeprecationTarget::Recipe(t) => {
                let mut new = (**t).clone();
                new.undeprecate()?;
                let _ = std::mem::replace(t, new.into());
            }
            DeprecationTarget::Package(t) => {
                let mut new = (**t).clone();
                new.undeprecate()?;
                let _ = std::mem::replace(t, new.into());
            }
        }
        Ok(())
    }
}

impl DeprecationTarget {
    fn ident(&self) -> AnyIdent {
        match self {
            DeprecationTarget::Recipe(r) => r.ident().to_any(None),
            DeprecationTarget::Package(r) => r.ident().to_any(),
        }
    }
}
