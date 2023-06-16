// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use async_stream::try_stream;
use clap::Args;
use colored::Colorize;
use futures::{Stream, TryStreamExt};
use itertools::Itertools;
use spfs::graph::Object;
use spfs::tracking::{DiskUsage, EntryDiskUsage, LEVEL_SEPARATOR};
use spfs::Digest;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::ident::parse_ident;
use spk_schema::ident_build::Build;
use spk_schema::ident_component::Component;
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{BuildIdent, Deprecate, Package, Spec};

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

#[derive(Clone, Debug)]
pub struct PackageDiskUsage {
    pkg: PkgNameBuf,
    version: Arc<Version>,
    build: Build,
    component: Component,
    entry: EntryDiskUsage,
    deprecated: bool,
    repo_name: String,
}

impl PackageDiskUsage {
    pub fn new(pkgname: PkgNameBuf, repo_name: String) -> Self {
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

    pub fn generate_partial_path(&self, depth: usize, ends_with_level_separator: bool) -> String {
        let full_path = self.generate_full_path();
        let mut full_path_by_level = full_path.split(LEVEL_SEPARATOR).collect_vec();
        full_path_by_level.retain(|c| !c.is_empty());

        let max_depth = if ends_with_level_separator {
            full_path_by_level.len() - 1
        } else {
            full_path_by_level.len()
        };
        let suffix = if depth.lt(&max_depth) { "/" } else { "" };
        if ends_with_level_separator {
            format!(
                "{}{}",
                full_path_by_level[..depth + 1].join(&LEVEL_SEPARATOR.to_string()),
                suffix
            )
        } else {
            format!(
                "{}{}",
                full_path_by_level[..depth].join(&LEVEL_SEPARATOR.to_string()),
                suffix
            )
        }
    }

    pub fn generate_full_path(&self) -> String {
        format!(
            "{}/{}/{}/{}/:{}/{}",
            self.repo_name,
            self.pkg,
            self.version,
            self.build,
            self.component.as_str(),
            self.entry.path(),
        )
    }

    pub fn get_size_of_entry(&self) -> u64 {
        self.entry.size()
    }

    pub fn get_package(&self) -> &PkgNameBuf {
        &self.pkg
    }

    pub fn get_version(&self) -> &Arc<Version> {
        &self.version
    }

    pub fn is_matching_path(&self, input: &String) -> bool {
        let path = self.generate_full_path();
        path.contains(input)
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated
    }

    pub fn update_deprecation_status(&mut self, deprecated: bool) {
        self.deprecated = deprecated
    }

    pub fn update_version(&mut self, version: Arc<Version>) {
        self.version = version
    }

    pub fn update_build(&mut self, build: &Build) {
        self.build = build.clone()
    }

    pub fn update_component(&mut self, component: Component) {
        self.component = component
    }

    pub fn update_disk_usage(&mut self, entry: EntryDiskUsage) {
        self.entry = entry
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
    #[clap(long, short = 'L')]
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
        let mut total_size = 0;
        let mut visited_digests = HashSet::new();
        if self.summarize {
            let mut input_by_level = self.path.split(LEVEL_SEPARATOR).rev().collect_vec();

            input_by_level.retain(|c| !c.is_empty());

            let input_depth = input_by_level.len();
            let mut grouped_entries: HashMap<String, (u64, &str)> = HashMap::new();
            let mut walked = self.walk();
            while let Some(du) = walked.try_next().await? {
                let deprecate = if du.is_deprecated() { "DEPRECATED" } else { "" };
                let partial_path =
                    du.generate_partial_path(input_depth, self.path.ends_with(LEVEL_SEPARATOR));
                let entry_size = if visited_digests.insert(*du.entry.digest()) || self.count_links {
                    du.get_size_of_entry()
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
        } else {
            let mut walked = self.walk();
            while let Some(du) = walked.try_next().await? {
                let full_path = du.generate_full_path();
                let deprecate = if du.is_deprecated() {
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
                    "{size:>WIDTH$}    {full_path} {deprecate}",
                    size = self.human_readable(entry_size),
                ));
                total_size += entry_size;
            }
        }

        if self.total {
            self.output.println(format!(
                "{:>WIDTH$}    total",
                self.human_readable(total_size)
            ));
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

    fn walk(&self) -> impl Stream<Item = Result<PackageDiskUsage>> + '_ {
        let mut input = self.path.split(LEVEL_SEPARATOR).rev().collect_vec();

        input.retain(|c| !c.is_empty());

        Box::pin(try_stream! {
            let repos = self.repos.get_repos_for_non_destructive_operation().await?;
            let input_repo = input.pop();
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

                let mut packages = self.walk_packages(input.pop(), repo_index);
                while let Some(pkg) = packages.try_next().await? {
                    let package_du = PackageDiskUsage::new(pkg, matched_repo_name.clone());
                    let mut versions = self.walk_versions(package_du.get_package(), repo_index, input.pop());
                    while let Some(version) = versions.try_next().await? {
                        let mut pkg_du_with_version = package_du.clone();
                        pkg_du_with_version.update_version(version);
                        let pkg_with_version = format!("{}/{}", pkg_du_with_version.get_package(), pkg_du_with_version.get_version());
                        let mut specs = self.walk_specs(&pkg_with_version, repo_index, input.pop());
                        while let Some(spec) = specs.try_next().await? {
                            let mut pkg_du_with_build = pkg_du_with_version.clone();
                            pkg_du_with_build.update_build(spec.ident().build());
                            pkg_du_with_build.update_deprecation_status(spec.is_deprecated());
                            let mut components = self.walk_components(spec.ident(), repo_index, input.pop());
                            while let Some((component, digest)) = components.try_next().await? {
                                let mut pkg_du_with_component = pkg_du_with_build.clone();
                                pkg_du_with_component.update_component(component);

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
                                                    pkg_du_with_entry.update_disk_usage(disk_usage);

                                                    if pkg_du_with_entry.is_matching_path(&self.path) {
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

            let builds = &mut repo.list_package_builds(pkg_ident.as_version()).await?;
            while let Some(build) = builds.pop() {
                if build.is_embedded() {
                    continue;
                }
                let spec = repo.read_package(&build).await?;
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
