// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use itertools::Itertools;
use spfs::graph::Object;
use spfs::storage::RepositoryHandle;
use spfs::tracking::{EntryDiskUsage, LEVEL_SEPARATOR};
use spfs::Digest;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::ident::parse_ident;
use spk_schema::name::PkgName;
use spk_schema::{Deprecate, Package, Spec};

const DIGEST_LEVEL: usize = 3;
const COMPONENT_LEVEL: usize = 4;

/// Abstract methods to keep track and update the
/// longest string length value between all entries
pub trait Output: Default + Send + Sync {
    /// Updates the largest string length value for printing
    fn update_string_length(&mut self, count: usize);

    /// Returns current longest string length for printing
    fn get_current_string_count(&mut self) -> usize;
}

/// Keeps track of the longest string length value
/// between all threads, to align the output of each print.
#[derive(Default)]
pub struct Console {
    pub longest_string_count: usize,
    pub input_level: usize,
}

impl Output for Console {
    fn update_string_length(&mut self, count: usize) {
        if count > self.longest_string_count {
            self.longest_string_count = count;
        }
    }

    fn get_current_string_count(&mut self) -> usize {
        self.longest_string_count
    }
}

/// Return the disk usage of a package
#[derive(Args)]
pub struct Du<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// The Package/Version to show the disk usage of
    #[clap(name = "PKG NAME/VERSION")]
    pub package: String,

    /// Count sizes many times if hard linked
    #[clap(long, short = 'L')]
    pub count_links: bool,

    /// Lists deprecated packages
    #[clap(long, short = 'd')]
    pub deprecated: bool,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    pub human_readable: bool,

    /// Shows each directory size from input package passed
    #[clap(long, short = 's')]
    pub short: bool,

    // Output the grand total
    #[clap(long, short = 'c')]
    pub total: bool,

    /// Output is updated while the command
    /// runs to update the longest length string
    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Du<T> {
    async fn run(&mut self) -> Result<i32> {
        let mut input_by_level: Vec<String> = self
            .package
            .split(LEVEL_SEPARATOR)
            .map(str::to_string)
            .collect();

        // Remove any empty strings
        input_by_level.retain(|c| !c.is_empty());

        let level: usize = input_by_level.len();

        let input_component = match level >= COMPONENT_LEVEL {
            true => input_by_level[3].to_string(),
            false => "".to_string(),
        };

        let input_digest = match level >= DIGEST_LEVEL {
            true => input_by_level[2].to_string(),
            false => "".to_string(),
        };

        let spfs_storage_dirs = match input_by_level.len() > COMPONENT_LEVEL {
            true => input_by_level[COMPONENT_LEVEL..].join(&LEVEL_SEPARATOR.to_string()),
            false => "".to_string(),
        };

        let specs = self.compile_entries_to_calculate(input_by_level).await?;

        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        let mut disk_usage: Vec<EntryDiskUsage> = Vec::new();

        for (_, repo) in repos.iter() {
            for spec in specs.iter() {
                if spec.is_deprecated() && !self.deprecated {
                    continue;
                }

                let components = match repo.read_components(spec.ident()).await {
                    Ok(c) => c,
                    _ => continue,
                };

                let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                let mut per_component_disk_usage: Vec<EntryDiskUsage> = Vec::new();

                for (component, digest) in components.iter().sorted_by_key(|(k, _)| *k) {
                    let mut component_for_output = ":".to_string();
                    component_for_output.push_str(component.as_str());

                    // If a digest or component is provided in the input argument, we only need the entry with the
                    // matching digest or component and can skip the rest.
                    let abs_path = format!("{}/{component_for_output}", spec.ident());
                    if !input_digest.is_empty() && !abs_path.contains(&input_digest) {
                        continue;
                    }

                    if !input_component.is_empty() && !abs_path.contains(&input_component) {
                        continue;
                    }

                    let mut component_du = self
                        .process_entry_size(digest, repo, &spfs_storage_dirs)
                        .await?;

                    component_du.pkg_info = abs_path.to_string();
                    component_du.deprecated = spec.is_deprecated();
                    component_du.calculate_total_size(&per_component_disk_usage, self.count_links);
                    per_component_disk_usage.push(component_du);
                }
                disk_usage.append(&mut per_component_disk_usage);
            }
        }

        let mut total_output_size = 0;
        let mut sum_of_sizes_by_package: HashMap<(String, bool), u64> = HashMap::default();
        for component_du in disk_usage.iter() {
            if level < COMPONENT_LEVEL && self.short {
                let mut output_pkg = component_du.pkg_info.split(LEVEL_SEPARATOR).collect_vec()
                    [..=level]
                    .join(&LEVEL_SEPARATOR.to_string());

                output_pkg.push_str(&LEVEL_SEPARATOR.to_string());
                sum_of_sizes_by_package
                    .entry((output_pkg, component_du.deprecated))
                    .and_modify(|size| *size += component_du.total_size)
                    .or_insert(component_du.total_size);
            } else {
                let formatted_entries = self.format_entries(component_du);
                if self.package.ends_with(LEVEL_SEPARATOR) {
                    sum_of_sizes_by_package.extend(formatted_entries);
                } else {
                    let mut sum_of_formatted_entries: HashMap<(String, bool), u64> =
                        HashMap::default();
                    sum_of_formatted_entries.insert(
                        (self.package.to_string(), component_du.deprecated),
                        formatted_entries.values().sum(),
                    );
                    sum_of_sizes_by_package.extend(sum_of_formatted_entries);
                }
            }
            total_output_size += component_du.total_size;
        }

        self.output
            .update_string_length(total_output_size.to_string().len());
        let longest_str_length = self.output.get_current_string_count();
        if !sum_of_sizes_by_package.is_empty() {
            for (name, size) in sum_of_sizes_by_package
                .iter()
                .sorted_by_key(|(k, _)| *k)
                .rev()
            {
                let deprecated = name.1;
                let pkgname = if deprecated {
                    format!("{} {}", name.0, "DEPRECATED".red())
                } else {
                    name.0.to_string()
                };
                println!(
                    "{size:>longest_str_length$} {entry}",
                    size = self.human_readable(*size),
                    entry = pkgname,
                );
            }
        }

        // Print total if sum_of_sizes_by_package is not empty and -c argument is passed.
        if self.total && !sum_of_sizes_by_package.is_empty() {
            println!(
                "{size:>longest_str_length$} total",
                size = self.human_readable(total_output_size)
            );
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

    fn update_string_length(&mut self, sizes: Vec<&u64>) {
        for size in sizes {
            let size = self.human_readable(*size);
            self.output.update_string_length(size.len());
        }
    }

    async fn compile_entries_to_calculate(
        &self,
        input_by_level: Vec<String>,
    ) -> Result<Vec<Arc<Spec>>> {
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        // Only need the PACKAGE/VERSION so we remove anything after the DIGEST_LEVEL
        let mut input_package = if input_by_level.len() >= DIGEST_LEVEL {
            input_by_level[..2].join(&LEVEL_SEPARATOR.to_string())
        } else {
            self.package.clone()
        };

        // Check if input package ends with a `/`
        let ends_with_level_separator = input_package.ends_with(LEVEL_SEPARATOR);

        // If input package ends with `/` we must remove to obtain ident
        let pkg_ident = if ends_with_level_separator {
            input_package.pop();
            parse_ident(&input_package)?
        } else {
            parse_ident(&input_package)?
        };

        let pkgname = PkgName::new(pkg_ident.name())?;

        let mut specs: Vec<Arc<Spec>> = Vec::new();

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

        match pkg_ident.version_and_build() {
            Some(input_version) => {
                versions.retain(|(version, _)| *version == input_version);
            }
            None => {
                // If input package does not end with a '/' then we need to output the highest version.
                if !ends_with_level_separator {
                    // There always should exist at least one version per package.
                    let highest_version = versions.first().unwrap().clone();

                    versions.retain(|(version, _)| highest_version.0 == *version);
                }
            }
        }

        for (version, repo_index) in versions {
            let (_, repo) = repos.get(repo_index).unwrap();

            let pkg_ident = parse_ident(format!("{pkgname}/{version}"))?;

            let builds = &mut repo.list_package_builds(pkg_ident.as_version()).await?;

            while let Some(build) = builds.pop() {
                // Skip embedded builds
                if build.is_embedded() {
                    continue;
                };

                let spec = repo.read_package(&build).await?;

                if !self.deprecated && spec.is_deprecated() {
                    continue;
                } else {
                    specs.push(spec.clone());
                };
            }
        }
        Ok(specs)
    }

    fn format_entries(&mut self, entry: &EntryDiskUsage) -> HashMap<(String, bool), u64> {
        let sum_by_dir = match self.short {
            true => entry.group_entries(self.package.ends_with(LEVEL_SEPARATOR)),
            false => entry.convert_child_entries_for_output(),
        };
        self.update_string_length(sum_by_dir.values().collect_vec());
        sum_by_dir
    }

    async fn process_entry_size(
        &mut self,
        digest: &Digest,
        repo: &RepositoryHandle,
        root_dir: &String,
    ) -> Result<EntryDiskUsage> {
        let mut item = repo.read_ref(digest.to_string().as_str()).await?;
        let mut items_to_process: Vec<spfs::graph::Object> = vec![item];
        let mut entires_to_print: EntryDiskUsage = EntryDiskUsage::new(String::new());

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
                        entires_to_print = match tracking_manifest.find_entry_by_string(root_dir) {
                            Some(root) => root.generate_entry_disk_usage(root_dir),
                            _ => continue,
                        };
                    }
                    Object::Tree(_) | Object::Mask | Object::Blob(_) => (), // Object needs to be a type that can obtain a manifest to evaluate.
                }
            }
            items_to_process = std::mem::take(&mut next_iter_objects);
        }
        Ok(entires_to_print)
    }
}
