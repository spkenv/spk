// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use anyhow::Result;
use clap::Args;
use itertools::Itertools;

/// List the contents of a committed directory
#[derive(Debug, Args)]
#[clap(visible_aliases = &["list-dir", "list"])]
pub struct CmdLs {
    /// List files on a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The tag or digest of the file tree to read from
    #[clap(value_name = "REF")]
    reference: spfs::tracking::EnvSpecItem,

    /// Recursively list all files and directories
    #[clap(long, short = 'R')]
    recursive: bool,

    /// Long listing format
    #[clap(short = 'l')]
    long: bool,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    human_readable: bool,

    /// The subdirectory to list
    #[clap(default_value = "/spfs")]
    path: String,

    /// Username of user who last modified the input reference
    #[clap(skip)]
    username: String,

    /// last modified date of the input reference
    #[clap(skip)]
    last_modified: String,
}

#[derive(Default, Debug, Clone)]
pub struct EntriesPerDir {
    pub dir_name: String,
    pub longest_length_str: usize,
    pub entries: HashMap<String, spfs::tracking::Entry>,
}

impl EntriesPerDir {
    pub fn new(dir: String, entries: HashMap<String, spfs::tracking::Entry>) -> Self {
        Self {
            dir_name: dir,
            longest_length_str: 0,
            entries,
        }
    }

    fn update_string_length(&mut self, count: usize) {
        if count > self.longest_length_str {
            self.longest_length_str = count;
        }
    }

    fn generate_child_entries(&mut self) -> Vec<(String, EntriesPerDir)> {
        let mut result: Vec<(String, EntriesPerDir)> = Vec::new();
        for (entry_name, entry) in self.entries.iter() {
            if entry.is_dir() {
                let new_dir: String = format!("{}/{entry_name}", self.dir_name);
                let new_dir_entry = EntriesPerDir::new(new_dir.clone(), entry.entries.clone());

                result.push((new_dir, new_dir_entry));
            }
        }
        result
    }
}

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let mut entries_to_print: Vec<(String, EntriesPerDir)> = Vec::new();

        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        match repo.read_tag_metadata(self.reference.as_str()).await {
            Some(tag) => {
                self.username = tag.username_without_org();
                self.last_modified = tag.time.format("%b %e %H:%M").to_string();
            }
            _ => tracing::warn!(
                "Unable to find the username and last modified fields of: {}",
                self.reference.as_str()
            ),
        }

        let item = repo.read_ref(self.reference.as_str()).await?;

        let path = self
            .path
            .strip_prefix("/spfs")
            .unwrap_or(&self.path)
            .to_string();

        let manifest = spfs::compute_object_manifest(item, &repo).await?;
        if let Some(root_entries) = manifest.list_entries_in_dir(path.as_str()) {
            let root_dir = ".";
            if self.recursive {
                let mut entries_to_process: Vec<(String, EntriesPerDir)> = Vec::new();
                let entries_in_root = EntriesPerDir::new(root_dir.to_string(), root_entries);
                entries_to_print.push((root_dir.to_string(), entries_in_root.clone()));
                entries_to_process.push((root_dir.to_string(), entries_in_root));

                while !entries_to_process.is_empty() {
                    let mut trees: Vec<(String, EntriesPerDir)> = Vec::new();
                    for (_, entries) in entries_to_process.iter_mut() {
                        trees.append(&mut entries.generate_child_entries());
                    }

                    entries_to_print.append(&mut trees.clone());
                    entries_to_process = std::mem::take(&mut trees);
                }

                // Update the longest length string for each EntriesPerDir
                for (_, entry) in entries_to_print.iter_mut() {
                    self.update_longest_length_string(entry);
                }
            } else {
                let mut entry: EntriesPerDir =
                    EntriesPerDir::new(root_dir.to_string(), root_entries);
                self.update_longest_length_string(&mut entry);
                entries_to_print.push((root_dir.to_string(), entry));
            }

            for (dir_name, entries) in entries_to_print.iter().sorted_by_key(|(k, _)| k) {
                if self.recursive {
                    println!("{}/:", dir_name);
                }
                self.print_entries_in_dir(entries);
            }
        } else {
            match manifest.get_path(path.as_str()) {
                None => {
                    tracing::error!("path not found in manifest: {}", self.path);
                }
                Some(_entry) => {
                    tracing::error!("path is not a directory: {}", self.path);
                }
            }
            return Ok(1);
        }
        Ok(0)
    }

    fn update_longest_length_string(&self, entry: &mut EntriesPerDir) {
        let mut longest_length_string = 0;
        for (_, e) in entry.entries.iter() {
            let size = self.human_readable(e.size);
            if size.len() > longest_length_string {
                longest_length_string = size.len();
            }
        }
        entry.update_string_length(longest_length_string);
    }

    fn human_readable(&self, size: u64) -> String {
        let mut result = size.to_string();
        if self.human_readable {
            result = spfs::io::format_size(size)
        }

        result
    }

    fn print_entries_in_dir(&mut self, entries: &EntriesPerDir) {
        let longest_length_str = entries.longest_length_str;
        for (name, entry) in entries.entries.iter().sorted_by_key(|(k, _)| *k) {
            let size: String = self.human_readable(entry.size);
            if self.long {
                match entry.kind {
                    spfs::tracking::EntryKind::Tree => {
                        println!(
                            "{} {username} {size:>longest_length_str$} {modified} {name}/",
                            unix_mode::to_string(entry.mode),
                            username = self.username,
                            modified = self.last_modified,
                        )
                    }
                    _ => {
                        println!(
                            "{} {username} {size:>longest_length_str$} {modified} {name}",
                            unix_mode::to_string(entry.mode),
                            username = self.username,
                            modified = self.last_modified,
                        )
                    }
                };
            } else if self.recursive {
                match entry.kind {
                    spfs::tracking::EntryKind::Tree => print!("{name}/  "),
                    _ => print!("{name}/  "),
                };
            } else {
                match entry.kind {
                    spfs::tracking::EntryKind::Tree => println!("{name}/"),
                    _ => println!("{name}"),
                };
            }
        }

        // Additional new lines needed for output
        if self.long && self.recursive {
            println!();
        } else if !self.long && self.recursive {
            println!("\n");
        }
    }
}
