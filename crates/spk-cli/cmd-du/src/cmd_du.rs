// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use async_stream::try_stream;
use clap::Args;
use colored::Colorize;
use futures::{Stream, TryStreamExt};
use itertools::Itertools;
use spfs::graph::Object;
use spfs::tracking::Entry;
use spfs::Digest;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::ident::parse_ident;
use spk_schema::ident_build::Build;
use spk_schema::ident_component::Component;
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{BuildIdent, Deprecate, Package, Spec};

use crate::entry_du::{DiskUsage, EntryDiskUsage, LEVEL_SEPARATOR};

const WIDTH: usize = 12;

#[cfg(test)]
#[path = "./cmd_du_test.rs"]
mod cmd_du_test;

/// Used for testing to compare the output of the results
pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&self, line: String);

    /// A line of output to display as a warning.
    fn warn(&self, line: String);
}

#[derive(Default)]
pub struct Console {}

impl Output for Console {
    fn println(&self, line: String) {
        println!("{line}");
    }

    fn warn(&self, line: String) {
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

    /// Returns true if the input is a subset of the absolute path.
    fn is_subset(&self, input: &[&str]) -> bool {
        let flatten_path = self.flatten_path();
        input
            .iter()
            .all(|item| flatten_path.contains(&item.to_string()))
    }
}

/// Return the disk usage of a package
#[derive(Args)]
pub struct Du<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// The path starting from repo name to calculate the disk usage
    #[clap(name = "REPO/PKG/VERSION/...")]
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
    async fn run(&mut self) -> Result<i32> {
        let input_depth = self.path.split(LEVEL_SEPARATOR).collect_vec().len();
        if self.summarize {
            self.print_grouped_entries(input_depth).await?;
        } else {
            self.print_all_entries(input_depth).await?;
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
    fn human_readable(&self, size: u64) -> String {
        if self.human_readable {
            spfs::io::format_size(size)
        } else {
            size.to_string()
        }
    }

    fn print_total(&self, total: u64) {
        if self.total {
            self.output
                .println(format!("{:>WIDTH$}    total", self.human_readable(total)));
        }
    }

    async fn print_all_entries(&self, input_depth: usize) -> Result<()> {
        let mut total_size = 0;
        let mut visited_digests = HashSet::new();
        let mut walked = self.walk();
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

            self.output.println(format!(
                "{size:>WIDTH$}    {joined_path} {deprecate}",
                size = self.human_readable(entry_size),
            ));

            total_size += entry_size;
        }

        self.print_total(total_size);
        Ok(())
    }

    async fn print_grouped_entries(&self, input_depth: usize) -> Result<()> {
        let mut total_size = 0;
        let mut visited_digests = HashSet::new();
        let mut grouped_entries: HashMap<String, (u64, &str)> = HashMap::new();
        let mut walked = self.walk();
        while let Some(du) = walked.try_next().await? {
            let deprecate = if du.deprecated { "DEPRECATED" } else { "" };
            let partial_path = match du.generate_partial_path(input_depth) {
                Some(path) => path,
                _ => continue,
            };

            let entry_size = if visited_digests.insert(*du.entry.digest()) || self.count_links {
                du.entry.size()
            } else {
                0 // 0 because we don't need to calculate sizes if its a duplicate or count_links is not enabled.
            };

            // If the partial path does not exist and grouped_entries is not empty,
            // the existing path is finished calculating and is ready to print.
            if !grouped_entries.contains_key(&partial_path) && !grouped_entries.is_empty() {
                for (path, (size, deprecate_status)) in grouped_entries.drain().take(1) {
                    self.output.println(format!(
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
            self.output.println(format!(
                "{size:>WIDTH$}    {path} {deprecate}",
                size = self.human_readable(*size),
                deprecate = deprecate_status.red(),
            ));
            total_size += size;
        }

        self.print_total(total_size);
        Ok(())
    }

    fn walk(&self) -> impl Stream<Item = Result<PackageDiskUsage>> + '_ {
        let mut input_path_in_parts: Vec<_> = self.path.split(LEVEL_SEPARATOR).rev().collect();

        input_path_in_parts.retain(|c| !c.is_empty());

        let mut input_to_eval = input_path_in_parts.clone();

        Box::pin(try_stream! {
            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let input_repo = input_to_eval.pop();
            for (repo_index, (repo_name, repo)) in repos.iter().enumerate() {
                let matched_repo_name = match input_repo {
                    Some(input_repo) => {
                        if repo_name == input_repo {
                            repo_name
                        } else {
                            continue
                        }
                    }
                    None => repo_name
                };

                let mut packages = self.walk_packages(input_to_eval.pop(), repo_index);
                while let Some(pkg) = packages.try_next().await? {
                    let package_du = PackageDiskUsage::new(pkg, matched_repo_name.clone());
                    let mut versions = self.walk_versions(&package_du.pkg, repo_index, input_to_eval.pop());
                    while let Some(version) = versions.try_next().await? {
                        let mut pkg_du_with_version = package_du.clone();
                        pkg_du_with_version.version = version;
                        let pkg_with_version = format!("{}/{}", pkg_du_with_version.pkg, &pkg_du_with_version.version);
                        let mut specs = self.walk_specs(&pkg_with_version, repo_index, input_to_eval.pop());
                        while let Some(spec) = specs.try_next().await? {
                            let mut pkg_du_with_build = pkg_du_with_version.clone();
                            pkg_du_with_build.build = spec.ident().build().clone();
                            pkg_du_with_build.deprecated = spec.is_deprecated();
                            let mut components = self.walk_components(spec.ident(), repo_index, input_to_eval.pop());
                            while let Some((component, digest)) = components.try_next().await? {
                                let mut pkg_du_with_component = pkg_du_with_build.clone();
                                pkg_du_with_component.component = component;

                                let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                                let mut item = repo.read_ref(digest.to_string().as_str()).await?;
                                let mut items_to_process: Vec<spfs::graph::Object> = vec![item];
                                while !items_to_process.is_empty() {
                                    let mut next_iter_objects: Vec<spfs::graph::Object> = Vec::new();
                                    for object in items_to_process.iter() {
                                        match object {
                                            Object::Platform(object) => {
                                                for reference in object.stack.iter() {
                                                    item = repo.read_ref(reference.to_string().as_str()).await?;
                                                    next_iter_objects.push(item);
                                                }
                                            }
                                            Object::Layer(object) => {
                                                item = repo.read_ref(object.manifest.to_string().as_str()).await?;
                                                next_iter_objects.push(item);
                                            }
                                            Object::Manifest(object) => {
                                                let tracking_manifest = object.to_tracking_manifest();
                                                let root_entry = tracking_manifest.take_root();
                                                let mut walked_entries = root_entry.walk();
                                                while let Some(disk_usage) = walked_entries.try_next().await? {
                                                    let mut pkg_du_with_entry = pkg_du_with_component.clone();
                                                    pkg_du_with_entry.entry = disk_usage;

                                                    if pkg_du_with_entry.is_subset(&input_path_in_parts) {
                                                        yield pkg_du_with_entry
                                                    }
                                                }
                                            }
                                            Object::Tree(_) => self.output.warn("Tree object cannot have disk usage generated".to_string()),
                                            Object::Blob(_) => self.output.warn("Blob object cannot have disk usage generated".to_string()),
                                            Object::Mask => ()
                                        }
                                    }
                                    items_to_process = std::mem::take(&mut next_iter_objects);
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    fn walk_packages<'a>(
        &'a self,
        input: Option<&'a str>,
        repo_index: usize,
    ) -> impl Stream<Item = Result<PkgNameBuf>> + 'a {
        Box::pin(try_stream! {
            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let (_, repo) = repos.get(repo_index).unwrap();
            for package in repo.list_packages().await? {
                match input {
                    Some(input_pkg) => {
                        if package == input_pkg {
                            yield package
                        }
                    }
                    None => yield package
                }
            }
        })
    }

    fn walk_versions<'a>(
        &'a self,
        pkg: &'a PkgNameBuf,
        repo_index: usize,
        input: Option<&'a str>,
    ) -> impl Stream<Item = Result<Arc<Version>>> + 'a {
        Box::pin(try_stream! {
            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let (_, repo) = repos.get(repo_index).unwrap();

            for version in repo.list_package_versions(pkg).await?.iter().rev() {
                match input {
                    Some(input_version) => {
                        if version.to_string() == input_version {
                            yield version.to_owned()
                        }
                    }
                    None => yield version.to_owned()
                }
            }
        })
    }

    fn walk_specs<'a>(
        &'a self,
        pkg_with_version: &'a String,
        repo_index: usize,
        input: Option<&'a str>,
    ) -> impl Stream<Item = Result<Arc<Spec>>> + 'a {
        Box::pin(try_stream! {
            let pkg_ident = parse_ident(pkg_with_version)?;

            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let (_, repo) = repos.get(repo_index).unwrap();

            let builds = repo.list_package_builds(pkg_ident.as_version()).await?;
            for build in builds.iter().sorted_by_key(|k| *k) {
                if build.is_embedded() {
                    continue;
                }
                let spec = repo.read_package(build).await?;
                if !self.deprecated && spec.is_deprecated() {
                    continue;
                }
                match input {
                    Some(input_build) => {
                        if spec.ident().build().to_string() == input_build {
                            yield spec
                        }
                    }
                    None => yield spec
                }
            }
        })
    }

    fn walk_components<'a>(
        &'a self,
        ident: &'a BuildIdent,
        repo_index: usize,
        input: Option<&'a str>,
    ) -> impl Stream<Item = Result<(Component, Digest)>> + 'a {
        Box::pin(try_stream! {
            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let (_, repo) = repos.get(repo_index).unwrap();

            let components = repo.read_components(ident).await?;
            for (component, digest) in components.iter().sorted_by_key(|(k, _)| *k) {
                match input {
                    Some(input_component) => {
                        if input_component.contains(&component.to_string()) {
                            yield (component.clone(), digest.clone())
                        }
                    }
                    None => yield (component.clone(), digest.clone())
                }
            }
        })
    }
}

impl DiskUsage for Entry {
    fn walk(&self) -> Pin<Box<dyn Stream<Item = Result<EntryDiskUsage>> + Send + Sync + '_>> {
        fn walk_nested_entries(
            root_entry: &Entry,
            parent_paths: Vec<Arc<String>>,
        ) -> Pin<Box<dyn Stream<Item = Result<EntryDiskUsage>> + Send + Sync + '_>> {
            Box::pin(try_stream! {
                for (path, entry) in root_entry.entries.iter() {

                    // Update path
                    let mut updated_paths = parent_paths.clone();
                    updated_paths.push(Arc::new(path.clone()));

                    // Base case. We can start traversing back up.
                    if entry.kind.is_blob() {
                        yield EntryDiskUsage::new(
                                updated_paths.clone(),
                                entry.size,
                                entry.object,
                            )
                    }

                    // We need to walk deeper if more child entries exists.
                    if !entry.entries.is_empty() {
                        for await du in walk_nested_entries(entry, updated_paths) {
                            yield du?;
                        }
                    }
                }
            })
        }

        Box::pin(try_stream! {
            // Sets up the initial paths before recursively walking all child entries.
            for (path, entry) in self.entries.iter().sorted_by_key(|(k, _)| *k) {
                for await du in walk_nested_entries(entry, vec![Arc::new(path.clone())]) {
                    yield du?;
                }
            }
        })
    }
}
