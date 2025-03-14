// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::fmt::Arguments;
use std::sync::Arc;

use async_stream::try_stream;
use clap::Args;
use colored::Colorize;
use futures::{Stream, TryStreamExt};
use itertools::Itertools;
use miette::Result;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::ident::parse_ident_range;
use spk_schema::ident_build::Build;
use spk_schema::ident_component::Component;
use spk_schema::ident_ops::parsing::{KNOWN_REPOSITORY_NAMES, repo_name_from_ident};
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{Deprecate, Package};
use spk_storage;
use spk_storage::walker::{RepoWalkerBuilder, RepoWalkerItem};

use crate::entry_du::{EntryDiskUsage, LEVEL_SEPARATOR};

// Number of characters disk size outputs are padded too
const WIDTH: usize = 12;

// Request and query path related constants
const COMPONENTS_MARKER: &str = "/:";
const COMPONENTS_SEPARATOR: &str = ":";
const REPO_INDEX: usize = 0;
const NAME_INDEX: usize = 1;
const VERSION_INDEX: usize = 2;
const BUILD_INDEX: usize = 3;

#[cfg(test)]
#[path = "./cmd_du_test.rs"]
mod cmd_du_test;

/// Used for testing to compare the output of the results
pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&self, line: Arguments);

    /// A line of output to display as a warning.
    fn warn(&self, line: Arguments);
}

#[derive(Default)]
pub struct Console {}

impl Output for Console {
    fn println(&self, line: Arguments) {
        println!("{line}");
    }

    fn warn(&self, line: Arguments) {
        tracing::warn!("{line}");
    }
}

/// Stores the values of the package being walked.
/// Starting from the repo name, it will store the
/// absolute path up to an entry blob from the /spfs dir.
#[derive(Clone, Debug)]
pub struct PackageDiskUsage {
    pub repo_name: String,
    pub pkg: PkgNameBuf,
    pub version: Arc<Version>,
    pub build: Build,
    pub component: Component,
    pub entry: EntryDiskUsage,
    pub deprecated: bool,
}

impl PackageDiskUsage {
    /// Construct an empty PackageDiskUsage
    fn new(pkgname: PkgNameBuf, repo_name: String) -> Self {
        Self {
            pkg: pkgname,
            version: Version::default().into(),
            build: Build::empty().clone(),
            component: Component::default_for_build(),
            entry: EntryDiskUsage::default(),
            deprecated: false,
            repo_name,
        }
    }

    /// Constructs a path from the values in PackageDiskUsage.
    fn flatten_path(&self) -> Vec<String> {
        let component = format!(":{}", self.component);
        let mut path = vec![
            self.repo_name.clone(),
            self.pkg.to_string(),
            self.version.to_string(),
            self.build.to_string(),
            component,
        ];

        self.entry
            .path()
            .iter()
            .for_each(|p| path.push(p.to_string()));

        path
    }

    /// Generates a partial path from the stored values from PackageDiskUsage given a depth.
    fn generate_partial_path(&self, depth: usize) -> Option<String> {
        let mut abs_path = self.flatten_path();
        let max_depth = abs_path.len();

        if depth.lt(&max_depth) {
            abs_path.truncate(depth);
            Some(format!(
                "{}{LEVEL_SEPARATOR}",
                abs_path.join(&LEVEL_SEPARATOR.to_string()),
            ))
        } else if depth.eq(&max_depth) {
            Some(abs_path.join(&LEVEL_SEPARATOR.to_string()))
        } else {
            None
        }
    }
}

/// Return the disk usage of a package
#[derive(Args)]
pub struct Du<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// Starting path to calculate the disk usage for. Can be
    /// either in path format,
    /// i.e. REPO/PKG/VERSION/BUILD/:COMPONENTS/dir/to/file, or
    /// package request format,
    /// i.e. REPO/PKG:COMPONENTS/VERSION/BUILD/dir/to/file
    #[clap(name = "REPO/PKG/VERSION/BUILD/:COMPONENTS/dir/to/file")]
    pub path: String,

    /// Count sizes many times if hard linked
    #[clap(long, short = 'l')]
    pub count_links: bool,

    /// Lists deprecated packages
    #[clap(long, short = 'd')]
    pub deprecated: bool,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    pub human_readable: bool,

    /// Display only a total for each argument
    #[clap(long, short = 's')]
    pub summarize: bool,

    /// Produce the grand total
    #[clap(long, short = 'c')]
    pub total: bool,

    /// Used for testing
    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Du<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        // Work out what the user is looking to limit the du too
        let (repo_name, package_path, file_path) = self.extract_names_and_path()?;

        let mut input_depth = self.path.split(LEVEL_SEPARATOR).collect_vec().len();
        if repo_name.is_none() {
            // Plus one for the missing repo level
            input_depth += 1;
        }
        if package_path.is_some()
            && !self.path.contains(COMPONENTS_MARKER)
            && self.path.contains(COMPONENTS_SEPARATOR)
        {
            // Components count as an extra level for the package
            // request format, but not the path format.
            input_depth += 1;
        }

        if let Some(ref rn) = repo_name {
            // There was a repo name given in the search path so
            // ensure that one is enabled and others are not
            tracing::debug!("Found repo name is du term: {rn}");
            self.repos.enable_repo = vec![rn.clone()];
            if rn == "local" {
                tracing::debug!("Limiting du to local repo");
                self.repos.local_repo_only = true;
            } else {
                tracing::debug!("Excluding local repo");
                self.repos.no_local_repo = true;
            }
        }
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        if self.summarize {
            self.print_grouped_entries(&repos, input_depth, package_path, file_path)
                .await?;
        } else {
            self.print_all_entries(&repos, input_depth, package_path, file_path)
                .await?;
        }
        Ok(0)
    }
}

impl<T: Output> CommandArgs for Du<T> {
    fn get_positional_args(&self) -> Vec<String> {
        Vec::new()
    }
}

impl<T: Output> Du<T> {
    // Examine the command line path argument and work out what it
    // represents. It could be a repo or package identifier, in either
    // package request or du components path format, with or without a
    // sub-component file path.
    //
    // Returns a tuple of a repo name, a package/request string, and a file path.
    //
    // TODO: maybe move to the spk library with du_walk, perhaps as a
    // parser in future.
    fn extract_names_and_path(
        &mut self,
    ) -> Result<(Option<String>, Option<String>, Option<String>)> {
        // Remove trailing /'s to simplify the path
        let trimmed_path = self.path.trim_end_matches("/");

        let (repository_name, package_path, file_path) =
            if KNOWN_REPOSITORY_NAMES.contains(trimmed_path) {
                // This is just a known repo name, it could also be a
                // package name, which is unlikely and ambiguous, so
                // treat it as a repo name.
                (Some(trimmed_path.to_string()), None, None)
            } else {
                match parse_ident_range(trimmed_path) {
                    Ok(mut pkg) => {
                        // It was in the package request format:
                        // repo/package:components/version/build/file/path/bits
                        let repo_name = pkg.repository_name.clone().map(|rn| rn.to_string());
                        pkg.repository_name = None;

                        (repo_name, Some(pkg.to_string()), None)
                    }
                    Err(e) => {
                        // It might be in path format,
                        // repo/package/version/build/:components/file/path,
                        // if it has a components section
                        if trimmed_path.contains(COMPONENTS_MARKER) {
                            // This has a components section in one of the path sub-directories.
                            // It is in the du path form, e.g.:
                            //    repo/pkg/version/build/:components/...
                            let parts: Vec<_> =
                                trimmed_path.split(&LEVEL_SEPARATOR.to_string()).collect();

                            // We checked for /: in the path before splitting it up
                            // so the unwrap should not fail.
                            let index = parts.iter().position(|&p| p.starts_with(":")).unwrap();

                            // The path will include at least pkg/ver/build/:run
                            // but it might not start with a repo name, or have
                            // a file path after the components section. Rearrange
                            // it into a package ident/request form to make it
                            // easier to check via the ident parser.
                            let possible_ident =
                                if KNOWN_REPOSITORY_NAMES.contains(&parts[REPO_INDEX]) {
                                    // This starts with a known repo
                                    let pkg_name_and_components =
                                        format!("{}{}", parts[NAME_INDEX], parts[index]);
                                    [
                                        parts[REPO_INDEX],
                                        &pkg_name_and_components,
                                        parts[VERSION_INDEX],
                                        parts[BUILD_INDEX],
                                    ]
                                    .join(&LEVEL_SEPARATOR.to_string())
                                } else {
                                    // This doesn't start with a known repo
                                    let pkg_name_and_components =
                                        format!("{}{}", parts[REPO_INDEX], parts[index]);
                                    [
                                        &pkg_name_and_components,
                                        parts[VERSION_INDEX],
                                        parts[BUILD_INDEX],
                                    ]
                                    .join(&LEVEL_SEPARATOR.to_string())
                                };

                            // Extract the repo name, if any, and parse the
                            // rest as a package ident.
                            let (rest, repository_name) =
                                match repo_name_from_ident::<nom_supreme::error::ErrorTree<_>>(
                                    &possible_ident,
                                    &KNOWN_REPOSITORY_NAMES,
                                ) {
                                    Ok((rest, rn)) => (rest.to_string(), rn),
                                    Err(_e) => (possible_ident.clone(), None),
                                };
                            let repo_name = repository_name.map(|rn| rn.to_string());

                            let ident = parse_ident_range(rest)?;

                            // All the pieces beyond the /: components
                            // section treated as a file path inside
                            // the package build (under the components).
                            let file_path = if index + 1 < parts.len() {
                                Some(parts[index + 1..].join(&LEVEL_SEPARATOR.to_string()))
                            } else {
                                None
                            };

                            (repo_name, Some(ident.to_string()), file_path)
                        } else {
                            // This doesn't parse as a request, so it's
                            // probably in path form, but it doesn't
                            // have a /:components section, so that's
                            // an error.
                            return Err(e.into());
                        }
                    }
                }
            };

        Ok((repository_name, package_path, file_path))
    }

    fn human_readable(&self, size: u64) -> String {
        if self.human_readable {
            spfs::io::format_size(size)
        } else {
            size.to_string()
        }
    }

    fn print_total(&self, total: u64) {
        if self.total {
            self.output.println(format_args!(
                "{:>WIDTH$}    total",
                self.human_readable(total)
            ));
        }
    }

    async fn print_all_entries(
        &self,
        repos: &Vec<(String, spk_storage::RepositoryHandle)>,
        input_depth: usize,
        package_path: Option<String>,
        file_path: Option<String>,
    ) -> Result<()> {
        let mut total_size = 0;
        let mut visited_digests = HashSet::new();

        let mut walked = self.du_walk(repos, package_path, file_path);
        while let Some(du) = walked.try_next().await? {
            let abs_path = du.flatten_path();
            if input_depth > abs_path.len() {
                continue;
            }

            let joined_path = abs_path.join(&LEVEL_SEPARATOR.to_string());
            let deprecate = if du.deprecated {
                "DEPRECATED".red()
            } else {
                "".into()
            };

            let entry_size = if visited_digests.insert(*du.entry.digest()) || self.count_links {
                du.entry.size()
            } else {
                0
            };

            self.output.println(format_args!(
                "{size:>WIDTH$}    {joined_path} {deprecate}",
                size = self.human_readable(entry_size),
            ));

            total_size += entry_size;
        }

        self.print_total(total_size);
        Ok(())
    }

    async fn print_grouped_entries(
        &self,
        repos: &Vec<(String, spk_storage::RepositoryHandle)>,
        input_depth: usize,
        package_path: Option<String>,
        file_path: Option<String>,
    ) -> Result<()> {
        let mut total_size = 0;
        let mut visited_digests = HashSet::new();
        let mut grouped_entries: HashMap<String, (u64, &str)> = HashMap::new();

        let mut walked = self.du_walk(repos, package_path, file_path);
        while let Some(du) = walked.try_next().await? {
            let deprecate = if du.deprecated { "DEPRECATED" } else { "" };
            let partial_path = match du.generate_partial_path(input_depth) {
                Some(path) => path,
                _ => continue,
            };

            let entry_size = if visited_digests.insert(*du.entry.digest()) || self.count_links {
                du.entry.size()
            } else {
                // Set to 0 because we don't need to calculate sizes
                // if it is a duplicate or count_links is not enabled.
                0
            };

            // If the partial path does not exist and grouped_entries is not empty,
            // the existing path is finished calculating and is ready to print.
            if !grouped_entries.contains_key(&partial_path) && !grouped_entries.is_empty() {
                for (path, (size, deprecate_status)) in grouped_entries.drain().take(1) {
                    self.output.println(format_args!(
                        "{size:>WIDTH$}    {path} {deprecate}",
                        size = self.human_readable(size),
                        deprecate = deprecate_status.red(),
                    ));
                    total_size += size;
                }
            }

            grouped_entries
                .entry(partial_path)
                .and_modify(|(size, _)| *size += entry_size)
                .or_insert((entry_size, deprecate));
        }

        // Need to clear the last object inside grouped_entries.
        for (path, (size, deprecate_status)) in grouped_entries.iter() {
            self.output.println(format_args!(
                "{size:>WIDTH$}    {path} {deprecate}",
                size = self.human_readable(*size),
                deprecate = deprecate_status.red(),
            ));
            total_size += size;
        }

        self.print_total(total_size);
        Ok(())
    }

    // TODO: move to the spk library so other things can use it?
    fn du_walk<'a>(
        &'a self,
        repos: &'a Vec<(String, spk_storage::RepositoryHandle)>,
        package_path: Option<String>,
        file_path: Option<String>,
    ) -> impl Stream<Item = Result<PackageDiskUsage>> + 'a {
        Box::pin(try_stream! {
            let mut repo_walker_builder = RepoWalkerBuilder::new(repos);
            let repo_walker = repo_walker_builder
                .try_with_package_equals(&package_path)?
                .with_report_on_versions(true)
                .with_report_on_builds(true)
                .with_report_src_builds(true)
                .with_report_deprecated_builds(self.deprecated)
                .with_report_embedded_builds(false)
                .with_report_on_components(true)
                .with_report_on_files(true)
                .with_file_path(file_path)
                .with_continue_on_error(true)
                .build();

            let mut traversal = repo_walker.walk();

            let mut current_repo_name = "";
            let mut pkg_name = Arc::new(PkgNameBuf::try_from("place-holder-package-name").unwrap());
            let mut version = Arc::from(Version::new(0, 0, 0));
            let mut build = Build::Source;
            let mut component_name = Component::Run;
            let mut is_deprecated = false;

            while let Some(item) = traversal.try_next().await? {
                match item {
                    RepoWalkerItem::Package(package) => {
                        current_repo_name = package.repo_name;
                        pkg_name = package.name;
                    }
                    RepoWalkerItem::Version(version_item) => {
                        version = Arc::from(version_item.ident.version().clone());
                    }
                    RepoWalkerItem::Build(build_item) => {
                        build = build_item.spec.ident().build().clone();
                        is_deprecated = build_item.spec.is_deprecated();
                    }
                    RepoWalkerItem::Component(component) => {
                        component_name = component.name;
                    },
                    RepoWalkerItem::File(file) => {
                        let disk_usage = EntryDiskUsage::new(
                            file.path_pieces.clone(),
                            file.entry.size(),
                            file.entry.object,
                        );

                        let mut du = PackageDiskUsage::new((*pkg_name).clone(), current_repo_name.to_string());
                        du.version = version.clone();
                        du.build = build.clone();
                        du.component = component_name.clone();
                        du.entry = disk_usage;
                        du.deprecated = is_deprecated;

                        yield du
                    },
                    _ => {}
                }
            }
        })
    }
}
