// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use spfs::graph::Object;
use spfs::io::DigestFormat;
use spfs::storage::RepositoryHandle;
use spfs::Digest;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::ident::parse_ident;
use spk_schema::ident_component::Component;
use spk_schema::name::PkgName;
use spk_schema::{Deprecate, Ident, Package, Spec};

pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&mut self, line: String);

    /// A line of output to display as a warning.
    fn warn(&mut self, line: String);

    /// Updates the char count for printing
    fn update_char_count(&mut self, count: usize);

    /// Returns current char count for printing
    fn get_current_char_count(&mut self) -> usize;
}

#[derive(Default)]
pub struct Console {
    char_count: usize,
}

impl Output for Console {
    fn println(&mut self, line: String) {
        println!("{line}");
    }

    fn warn(&mut self, line: String) {
        tracing::warn!("{line}");
    }

    fn update_char_count(&mut self, count: usize) {
        if count > self.char_count {
            self.char_count = count;
        }
    }

    fn get_current_char_count(&mut self) -> usize {
        self.char_count
    }
}
/// Return the disk utility of a package version
#[derive(Args)]
pub struct Du<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// The Package/Version to show the disk utility of
    #[clap(name = "PKG NAME/VERSION")]
    pub package: String,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    pub human_readable: bool,

    /// Shows each directory size from input package passed
    #[clap(long, short = 's')]
    pub short: bool,

    // Output the grand total
    #[clap(long, short = 'c')]
    pub total: bool,

    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Du<T> {
    async fn run(&mut self) -> Result<i32> {
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        let mut package_path: Vec<String> = self.package.split('/').map(str::to_string).collect();

        let mut package = self.package.clone();
        // Remove any empty strings
        package_path.retain(|c| !c.is_empty());

        let mut longest_char = 0;
        let mut to_print: Vec<String> = Vec::new();
        if package_path.len() == 1 && !self.package.ends_with('/') {
            let pkgname = PkgName::new(&package)?;

            for (_repo_name, repo) in repos.iter() {
                let version = match repo.highest_package_version(pkgname).await? {
                    Some(v) => v,
                    _ => continue,
                };

                let mut name = String::from(&self.package);
                name.push('/');
                name.push_str(&version.to_string());

                let pkg_ident = parse_ident(name.clone())?;

                let mut builds = repo.list_package_builds(&pkg_ident).await?;

                let specs = self.get_specs_to_process(&mut builds, repo).await?;

                if !specs.is_empty() {
                    let components_to_process =
                        self.get_components_to_process(specs, repo, None).await?;

                    let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                    let mut total_size = 0;
                    for components in components_to_process.iter() {
                        let mut prev_digests: Vec<String> = Vec::new();
                        for (_, digest) in components.iter() {
                            if prev_digests.contains(&digest.to_string()) {
                                continue;
                            }
                            let (size, mut temp_to_print) = self
                                .process_component_size(digest, repo, None, None)
                                .await?;
                            if !self.short {
                                to_print.append(&mut temp_to_print);
                            }
                            total_size += size;
                            prev_digests.push(digest.to_string());
                        }
                    }

                    let size_to_print = self.human_readable(total_size);
                    if size_to_print.len() > longest_char {
                        longest_char = size_to_print.len();
                    }

                    if self.short {
                        to_print.push(format!("{size_to_print}-{name}"));
                    }

                    if self.total {
                        to_print.push(format!("{size_to_print}-total"));
                    }
                }
            }
        } else if package_path.len() == 1 && self.package.ends_with('/') {
            package.pop();

            let pkgname = PkgName::new(&package)?;

            let mut versions = Vec::new();
            for (index, (_, repo)) in repos.iter().enumerate() {
                versions.extend(
                    repo.list_package_versions(pkgname)
                        .await?
                        .iter()
                        .map(|v| ((**v).clone(), index)),
                );
            }

            versions.sort_by_key(|v| v.0.clone());
            versions.reverse();

            let mut total_size = 0;
            for (version, repo_index) in versions {
                let (_repo_name, repo) = repos.get(repo_index).unwrap();

                let mut name = String::from(&package);
                name.push('/');
                name.push_str(&version.to_string());

                let pkg_ident = parse_ident(name.clone())?;

                let mut builds = repo.list_package_builds(&pkg_ident).await?;

                let specs = self.get_specs_to_process(&mut builds, repo).await?;

                if !specs.is_empty() {
                    let components_to_process =
                        self.get_components_to_process(specs, repo, None).await?;

                    let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                    let mut per_version_size = 0;
                    for components in components_to_process.iter() {
                        let mut prev_digests: Vec<String> = Vec::new();
                        for (_, digest) in components.iter() {
                            if prev_digests.contains(&digest.to_string()) {
                                continue;
                            }
                            let (size, mut temp_to_print) = self
                                .process_component_size(digest, repo, None, None)
                                .await?;
                            if !self.short {
                                to_print.append(&mut temp_to_print);
                            }
                            per_version_size += size;
                            prev_digests.push(digest.to_string());
                        }
                    }

                    total_size += per_version_size;
                    let size_to_print = self.human_readable(per_version_size);
                    self.output.update_char_count(size_to_print.len());

                    if self.short {
                        to_print.push(format!("{size_to_print}-{name}/"));
                    }
                }
            }

            let size_to_print = self.human_readable(total_size);
            self.output.update_char_count(size_to_print.len());

            if self.total {
                to_print.push(format!("{size_to_print}-total"));
            }
        } else {
            let mut dir_to_check = None;
            let mut input_digest = None;
            if package_path.len() >= 3 {
                let (temp_package, temp_dir_to_check) = package_path.split_at(3);
                input_digest = temp_package.last();
                package = temp_package.join("/");
                if temp_dir_to_check.is_empty() {
                    dir_to_check = None;
                } else {
                    dir_to_check = Some(temp_dir_to_check.join("/"));
                }
            }

            if package.ends_with('/') {
                package.pop();
            }
            for (_repo_name, repo) in repos.iter() {
                let pkg_ident = parse_ident(&package)?;

                let mut builds = repo.list_package_builds(&pkg_ident).await?;

                let specs = self.get_specs_to_process(&mut builds, repo).await?;

                if !specs.is_empty() {
                    let components_to_process = self
                        .get_components_to_process(specs, repo, input_digest)
                        .await?;

                    let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                    let mut total_size = 0;
                    for components in components_to_process.iter() {
                        let mut prev_digests: Vec<String> = Vec::new();
                        for (_, digest) in components.iter().sorted_by_key(|(k, _)| *k) {
                            if prev_digests.contains(&digest.to_string()) {
                                continue;
                            }
                            if package_path.len() < 3 {
                                if self.package.ends_with('/') {
                                    let shortened_digest = match spfs::io::format_digest(
                                        *digest,
                                        DigestFormat::Shortened(repo),
                                    )
                                    .await
                                    {
                                        Ok(d) => d,
                                        Err(_) => "".to_string(),
                                    };

                                    let pkgname = format!("{package}/{shortened_digest}");

                                    let (size, _) = self
                                        .process_component_size(
                                            digest,
                                            repo,
                                            None,
                                            dir_to_check.as_ref(),
                                        )
                                        .await?;

                                    if self.short {
                                        to_print.push(format!(
                                            "{}-{}/",
                                            self.human_readable(size),
                                            pkgname,
                                        ));
                                    }
                                    total_size += size;
                                } else {
                                    let (size, _) = self
                                        .process_component_size(
                                            digest,
                                            repo,
                                            None,
                                            dir_to_check.as_ref(),
                                        )
                                        .await?;
                                    total_size += size;
                                }
                            } else {
                                let (size, mut temp_to_print) = self
                                    .process_component_size(
                                        digest,
                                        repo,
                                        Some(&package),
                                        dir_to_check.as_ref(),
                                    )
                                    .await?;
                                total_size += size;
                                to_print.append(&mut temp_to_print);
                            }
                            prev_digests.push(digest.to_string());
                        }
                    }

                    if (!self.package.ends_with('/') && package_path.len() < 3) && self.short {
                        to_print.push(format!("{}-{}/", self.human_readable(total_size), package));
                    }

                    if self.total {
                        to_print.push(format!("{}-total", self.human_readable(total_size)));
                    }
                }
            }
        }

        let longest_char_count = self.output.get_current_char_count();
        for output in to_print.iter() {
            if let Some((size, entry)) = output.split_once('-') {
                println!("{size:>longest_char_count$} {entry}");
            }
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
        let mut result = size.to_string();
        if self.human_readable {
            result = spfs::io::format_size(size)
        }

        result
    }

    async fn get_specs_to_process(
        &self,
        builds: &mut Vec<Ident>,
        repo: &spk_storage::RepositoryHandle,
    ) -> Result<Vec<Arc<Spec>>> {
        let mut result: Vec<Arc<Spec>> = Vec::new();
        builds.sort();
        while let Some(build) = builds.pop() {
            match repo.read_package(&build).await {
                Ok(spec) if !spec.is_deprecated() => result.push(spec),
                Ok(_) => {
                    continue;
                }
                Err(err) => {
                    println!("{}", format_args!("Error reading spec for {build}: {err}"));
                }
            }
        }
        Ok(result)
    }

    async fn get_components_to_process(
        &self,
        spec: Vec<Arc<Spec>>,
        repo: &spk_storage::RepositoryHandle,
        input_digest: Option<&String>,
    ) -> Result<Vec<HashMap<Component, Digest>>> {
        let mut result: Vec<HashMap<Component, Digest>> = Vec::new();
        for spec in spec.iter() {
            let ident = spec.ident();
            match repo.read_components(ident).await {
                Ok(c) => match input_digest {
                    Some(digest) => {
                        if c.values().any(|&x| x.to_string().contains(digest)) {
                            result.push(c);
                            return Ok(result);
                        }
                    }
                    _ => result.push(c),
                },
                Err(spk_storage::Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(_),
                )) => {
                    tracing::info!("Skipping missing build {ident}; currently being built?");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
        }

        Ok(result)
    }

    async fn process_component_size(
        &mut self,
        digest: &Digest,
        repo: &RepositoryHandle,
        pkgname: Option<&String>,
        dir_to_check: Option<&String>,
    ) -> Result<(u64, Vec<String>)> {
        let mut total_size = 0;
        let mut item = repo.read_ref(digest.to_string().as_str()).await?;
        let mut items_to_process: Vec<spfs::graph::Object> = vec![item];

        let root_dir = match dir_to_check {
            Some(dir) => format!("{dir}/"),
            _ => "".to_string(),
        };

        let name = match pkgname {
            Some(name) => name.as_str(),
            _ => "",
        };

        let path = [name, &root_dir].join("/");
        let mut to_print: Vec<String> = Vec::new();
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
                        let tracking_manifest = object.unlock();

                        if self.package.ends_with('/') {
                            let mut entries =
                                tracking_manifest.list_entries_in_dir(root_dir.as_str());
                            entries.sort();
                            for entry_name in entries {
                                let entry = tracking_manifest
                                    .find_entry_by_string(&format!("{root_dir}{entry_name}"));
                                if entry.is_regular_file() {
                                    total_size += entry.size;
                                    if !name.is_empty() {
                                        let size_to_print = self.human_readable(entry.size);
                                        self.output.update_char_count(size_to_print.len());
                                        to_print.push(format!(
                                            "{size_to_print}-{name}/{root_dir}{entry_name}"
                                        ));
                                    }
                                } else {
                                    let (size, mut temp_to_print, temp_longest_char) = entry
                                        .calculate_size_of_child_entries(
                                            self.short,
                                            root_dir.as_str(),
                                            self.human_readable,
                                            path.as_str(),
                                        );
                                    self.output.update_char_count(temp_longest_char);
                                    total_size += size;
                                    if !name.is_empty() {
                                        let size_to_print = self.human_readable(size);
                                        self.output.update_char_count(size_to_print.len());
                                        to_print.push(format!(
                                            "{size_to_print}-{name}/{root_dir}{entry_name}/"
                                        ));
                                    }
                                    if !self.short {
                                        to_print.append(&mut temp_to_print);
                                    }
                                }
                            }
                        } else {
                            let root_entry =
                                tracking_manifest.find_entry_by_string(root_dir.as_str());
                            if root_entry.is_regular_file() {
                                total_size += root_entry.size;
                                if !name.is_empty() {
                                    let size_to_print = self.human_readable(root_entry.size);
                                    self.output.update_char_count(size_to_print.len());
                                    to_print.push(format!("{size_to_print}-{name}/{root_dir}"));
                                }
                            } else {
                                let (size, mut temp_to_print, temp_longest_char) = root_entry
                                    .calculate_size_of_child_entries(
                                        self.short,
                                        root_dir.as_str(),
                                        self.human_readable,
                                        path.as_str(),
                                    );
                                total_size += size;
                                let size_to_print = self.human_readable(size);
                                self.output.update_char_count(temp_longest_char);

                                if !self.short {
                                    to_print.append(&mut temp_to_print);
                                }

                                if (!name.is_empty() || !root_dir.is_empty()) && self.short {
                                    to_print.push(format!("{size_to_print}-{name}/{root_dir}"));
                                }
                            }
                        }
                    }
                    Object::Tree(object) => {
                        for entry in object.entries.iter() {
                            total_size += entry.size;
                        }
                    }
                    Object::Blob(object) => {
                        total_size += object.size;
                    }
                    Object::Mask => (),
                }
            }
            items_to_process = std::mem::take(&mut next_iter_objects);
        }
        Ok((total_size, to_print))
    }
}
