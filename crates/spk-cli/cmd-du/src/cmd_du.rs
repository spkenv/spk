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
use spfs::io::DigestFormat;
use spfs::storage::RepositoryHandle;
use spfs::tracking::{EntryDiskUsage, LEVEL_SEPARATOR};
use spfs::Digest;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::ident::parse_ident;
use spk_schema::name::PkgName;
use spk_schema::{Deprecate, Package, Spec};

const PACKAGE_LEVEL: usize = 1;
const VERSION_LEVEL: usize = 2;
const DIGEST_LEVEL: usize = 3;

/// Abstract methods to keep track and update the
/// longest string length value between all entries
pub trait Output: Default + Send + Sync {
    /// A line of output to display.
    fn println(&mut self, line: String);

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
}

impl Output for Console {
    fn println(&mut self, line: String) {
        println!("{line}");
    }

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

/// Configurations needed when printing the
/// disk usage of an entry.
#[derive(Debug, Clone)]
pub struct EntryPrintConfig {
    pub spec: Arc<Spec>,
    pub deprecated: bool,
    pub total_size: u64,
    pub entries_to_print: Vec<(String, String)>,
}

impl EntryPrintConfig {
    pub fn new(spec: Arc<Spec>) -> Self {
        Self {
            spec,
            deprecated: false,
            total_size: 0,
            entries_to_print: Vec::new(),
        }
    }

    fn print_stored_entries(&mut self, longest_str_length: usize, print_deprecate_status: bool) {
        for (size, path) in self.entries_to_print.iter().sorted_by_key(|(_, k)| k) {
            let pkg_path = if print_deprecate_status && self.deprecated {
                format!("{path} {}", "DEPRECATED".red())
            } else {
                path.to_string()
            };
            println!("{size:>longest_str_length$} {pkg_path}", size = size);
        }
    }
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

        let level = input_by_level.len();

        let input_digest = if level >= DIGEST_LEVEL {
            input_by_level[2].clone()
        } else {
            "".to_string()
        };

        let spfs_storage_dirs = if input_by_level.len() > DIGEST_LEVEL {
            input_by_level[DIGEST_LEVEL..].join(&LEVEL_SEPARATOR.to_string())
        } else {
            "".to_string()
        };

        let mut entries = self.compile_entries_to_calculate(input_by_level).await?;

        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        entries.iter().sorted_by_key(|(k, _)| k);

        let mut formatted_entries: Vec<(String, EntryPrintConfig)> = Vec::new();

        for (_, repo) in repos.iter() {
            for (pkg_name, entry) in entries.iter_mut() {
                if entry.deprecated && !self.deprecated {
                    continue;
                }

                let components = match repo.read_components(entry.spec.ident()).await {
                    Ok(c) => c,
                    _ => continue,
                };

                let digests: Vec<_> = components.values().unique().collect();

                let spk_storage::RepositoryHandle::SPFS(repo) = repo else { continue; };

                for digest in digests.iter() {
                    let shortened_digest =
                        spfs::io::format_digest(**digest, DigestFormat::Shortened(repo)).await?;

                    let pkg_with_digest = [pkg_name.to_string(), shortened_digest.to_string()]
                        .join(&LEVEL_SEPARATOR.to_string());

                    // If a digest is provided in the input argument, we only need the entry with the
                    // matching digest and can skip the rest.
                    if !input_digest.is_empty() && !pkg_with_digest.contains(&input_digest) {
                        continue;
                    }

                    let (digest_size, mut to_print) = self
                        .process_entry_size(
                            digest,
                            repo,
                            &pkg_with_digest,
                            spfs_storage_dirs.as_str(),
                        )
                        .await?;

                    entry.total_size += digest_size;
                    entry.entries_to_print.append(&mut to_print);

                    let name = if self.skip_update_to_package_name(level) {
                        pkg_name.to_string()
                    } else {
                        pkg_with_digest
                    };
                    formatted_entries.push((name, entry.to_owned()));
                }
            }
        }

        let mut total_output_size = 0;
        let mut sum_of_sizes_by_package: HashMap<String, u64> = HashMap::default();
        for (pkg_name, entry) in formatted_entries
            .iter_mut()
            .sorted_by_key(|(k, _)| k.to_string())
        {
            if self.print_all_files(level) {
                entry.print_stored_entries(self.output.get_current_string_count(), self.deprecated);
            } else {
                let name = if entry.deprecated && self.deprecated {
                    format!("{pkg_name}/ {}", "DEPRECATED".red())
                } else {
                    format!("{pkg_name}/")
                };

                sum_of_sizes_by_package
                    .entry(name)
                    .and_modify(|size| *size += entry.total_size)
                    .or_insert(entry.total_size);
            }
            total_output_size += entry.total_size;
        }

        let longest_str_length = self.output.get_current_string_count();
        if !sum_of_sizes_by_package.is_empty() {
            for (name, size) in sum_of_sizes_by_package
                .iter()
                .sorted_by_key(|(k, _)| *k)
                .rev()
            {
                println!(
                    "{size:>longest_str_length$} {entry}",
                    size = self.human_readable(*size),
                    entry = name,
                );
            }
        }

        if self.total {
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

    fn print_all_files(&self, level: usize) -> bool {
        !self.short || level >= DIGEST_LEVEL
    }

    fn skip_update_to_package_name(&self, level: usize) -> bool {
        (level == VERSION_LEVEL && !self.package.ends_with(LEVEL_SEPARATOR))
            || level == PACKAGE_LEVEL
    }

    fn generate_entry_to_print(&mut self, size: u64, path: String) -> (String, String) {
        let size = self.human_readable(size);
        self.output.update_string_length(size.len());

        (size, path)
    }

    async fn compile_entries_to_calculate(
        &self,
        input_by_level: Vec<String>,
    ) -> Result<Vec<(String, EntryPrintConfig)>> {
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

        let mut specs: Vec<(String, EntryPrintConfig)> = Vec::new();

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

                let mut entry = EntryPrintConfig::new(spec.clone());

                entry.deprecated = spec.is_deprecated();

                if !self.deprecated && spec.is_deprecated() {
                    continue;
                } else {
                    specs.push((spec.ident().as_version().to_string(), entry))
                };
            }
        }
        Ok(specs)
    }

    fn format_entries_to_print(
        &mut self,
        root: String,
        child_entires: Vec<EntryDiskUsage>,
    ) -> Vec<(String, String)> {
        let mut to_print: Vec<(String, String)> = Vec::new();
        if !self.short {
            for entry in child_entires.iter() {
                if entry.child_entries.is_empty() {
                    to_print.push(
                        self.generate_entry_to_print(entry.total_size, entry.root.to_string()),
                    )
                } else {
                    for (size, path) in entry.child_entries.iter() {
                        to_print.push(self.generate_entry_to_print(*size, path.to_string()))
                    }
                }
            }
        } else if self.package.ends_with(LEVEL_SEPARATOR) {
            for entry in child_entires.iter() {
                to_print
                    .push(self.generate_entry_to_print(entry.total_size, entry.root.to_string()))
            }
        } else {
            let mut total_size = 0;
            for entry in child_entires.iter() {
                total_size += entry.total_size;
            }
            to_print.push(self.generate_entry_to_print(total_size, root));
        }
        to_print
    }

    async fn process_entry_size(
        &mut self,
        digest: &Digest,
        repo: &RepositoryHandle,
        pkg_path: &str,
        root_dir: &str,
    ) -> Result<(u64, Vec<(String, String)>)> {
        let mut item = repo.read_ref(digest.to_string().as_str()).await?;
        let mut items_to_process: Vec<spfs::graph::Object> = vec![item];
        let mut entires_to_print: Vec<(String, String)> = Vec::new();
        let mut child_entries: Vec<EntryDiskUsage> = Vec::new();

        let mut total_size = 0;
        let abs_root_path = [pkg_path, root_dir].join(&LEVEL_SEPARATOR.to_string());

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
                        let mut root = root_dir.to_string();
                        let root_entry = tracking_manifest.find_entry_by_string(&root);
                        let is_blob: bool = root_entry.kind.is_blob();

                        // If the input is a blob, we must search from the directory that contains the blob.
                        // Else, return the directories that exists in the root dir.
                        let entries = if is_blob {
                            let mut dir_containing_blob: Vec<String> =
                                root.split(LEVEL_SEPARATOR).map(str::to_string).collect();

                            dir_containing_blob.pop();
                            root = dir_containing_blob.join(&LEVEL_SEPARATOR.to_string());
                            tracking_manifest.list_entries_in_dir(&root)
                        } else {
                            tracking_manifest.list_entries_in_dir(&root)
                        };

                        // Loop through each child dir that exists in the root dir.
                        for entry_name in entries.iter().sorted_by_key(|k| **k) {
                            // If input entry is a blob, we only care about evaluating the target blob.
                            if is_blob && root_dir != format!("{root}/{entry_name}") {
                                continue;
                            }

                            // The absolute path including PACKAGE/VERSION/DIGEST.
                            let mut abs_path_vec = vec![pkg_path, &root, entry_name];
                            abs_path_vec.retain(|c| !c.is_empty());
                            let abs_path = abs_path_vec.join(&LEVEL_SEPARATOR.to_string());

                            // The path from the /spfs directory to find the target entry.
                            let mut target_entry_vec =
                                vec![root.to_string(), entry_name.to_string()];
                            target_entry_vec.retain(|c| !c.is_empty());
                            let target_entry = target_entry_vec.join(&LEVEL_SEPARATOR.to_string());

                            let entry = tracking_manifest.find_entry_by_string(&target_entry);
                            let entry_du = entry.generate_dir_disk_usage(&abs_path);
                            total_size += entry_du.total_size;
                            child_entries.push(entry_du);
                        }

                        entires_to_print.append(&mut self.format_entries_to_print(
                            abs_root_path.to_owned(),
                            child_entries.to_owned(),
                        ));
                    }
                    Object::Tree(_) | Object::Mask | Object::Blob(_) => (), // Object needs to be a type that can obtain a manifest to evaluate.
                }
            }
            items_to_process = std::mem::take(&mut next_iter_objects);
        }
        Ok((total_size, entires_to_print))
    }
}
