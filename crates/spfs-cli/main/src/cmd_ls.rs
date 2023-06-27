// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use spfs::tracking::{Entry, EnvSpecItem};

/// List the contents of a committed directory
#[derive(Debug, Args)]
#[clap(visible_aliases = &["list-dir", "list"])]
pub struct CmdLs {
    /// List files on a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The tag or digest of the file tree to read from
    #[clap(value_name = "REF")]
    reference: EnvSpecItem,

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

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        match &self.reference {
            EnvSpecItem::TagSpec(tag_spec) => {
                let tag = repo.resolve_tag(tag_spec).await?;
                self.username = tag.username_without_org().to_string();
                self.last_modified = tag.time.format("%b %e %H:%M").to_string();
            }
            EnvSpecItem::PartialDigest(_) | EnvSpecItem::Digest(_) => (),
        }

        let item = repo.read_ref(&self.reference.to_string()).await?;

        let path = self
            .path
            .strip_prefix("/spfs")
            .unwrap_or(&self.path)
            .to_string();

        let manifest = spfs::compute_object_manifest(item, &repo).await?;

        if let Some(root_entries) = manifest.list_entries_in_dir(path.as_str()) {
            if self.recursive {
                let mut entries_to_process: Vec<(String, HashMap<String, Entry>)> = Vec::new();
                entries_to_process.push((String::from("."), root_entries.clone()));

                while !entries_to_process.is_empty() {
                    let mut trees: Vec<(String, HashMap<String, Entry>)> = Vec::new();
                    for (dir, entries) in entries_to_process.iter() {
                        println!("{dir}:");
                        let print_width = entries
                            .iter()
                            .map(|(_, e)| self.human_readable(e.total_size()).len())
                            .max()
                            .unwrap_or(0);

                        for (path, entry) in entries.iter().sorted_by_key(|(k, _)| *k) {
                            self.print_entries_in_dir(path, entry, print_width);

                            if !entry.entries.is_empty() {
                                trees.push((format!("{dir}/{path}"), entry.entries.clone()));
                            }
                        }

                        println!();

                        // Additional new line needed when -l flag is not present.
                        // When -l flag is enabled, each entry is outputted on a new line.
                        // Whereas, when its disabled, each entry per dir is outputted on the same line.
                        if !self.long && self.recursive {
                            println!();
                        }
                    }
                    entries_to_process = std::mem::take(&mut trees);
                }
            } else {
                let print_width = root_entries
                    .iter()
                    .map(|(_, e)| self.human_readable(e.total_size()).len())
                    .max()
                    .unwrap_or(0);

                for (path, entry) in root_entries.iter().sorted_by_key(|(k, _)| *k) {
                    self.print_entries_in_dir(path, entry, print_width);
                }
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

    fn human_readable(&self, size: u64) -> String {
        if self.human_readable {
            spfs::io::format_size(size)
        } else {
            size.to_string()
        }
    }

    fn print_entries_in_dir(&mut self, dir: &String, entry: &Entry, width: usize) {
        let size: String = self.human_readable(entry.total_size());
        let suffix = if entry.kind.is_tree() { "/" } else { "" };
        if self.long {
            println!(
                "{} {username} {size:>width$} {modified} {dir}{suffix}",
                unix_mode::to_string(entry.mode),
                username = self.username,
                modified = self.last_modified,
            );
        } else if self.recursive {
            print!("{dir}{suffix}  ")
        } else {
            println!("{dir}{suffix}")
        }
    }
}

pub trait TotalSize {
    fn total_size(&self) -> u64;
}

impl TotalSize for Entry {
    fn total_size(&self) -> u64 {
        if self.is_dir() {
            self.entries.values().map(|e| e.total_size()).sum()
        } else {
            self.size
        }
    }
}
