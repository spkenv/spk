// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
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

    /// The subdirectory to list
    #[clap(default_value = "/spfs")]
    path: String,
}

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let item = repo.read_ref(&self.reference.to_string()).await?;

        let path = self
            .path
            .strip_prefix("/spfs")
            .unwrap_or(&self.path)
            .to_string();
        let manifest = spfs::compute_object_manifest(item, &repo).await?;
        if let Some(entries) = manifest.list_dir_verbose(path.as_str()) {
            if self.recursive {
                if let Some(entries) = manifest.list_dir_verbose(path.as_str()) {
                    let mut entries_to_process: HashMap<
                        String,
                        &HashMap<String, spfs::tracking::Entry>,
                    > = HashMap::new();
                    entries_to_process.insert(".".to_string(), entries);
                    while !entries_to_process.is_empty() {
                        let mut trees: HashMap<String, &HashMap<String, spfs::tracking::Entry>> =
                            HashMap::new();
                        for dir in entries_to_process.keys().sorted() {
                            println!("{dir}:");
                            for entry_name in entries_to_process[dir].keys().sorted() {
                                if let Some(entry) = entries_to_process[dir].get(entry_name) {
                                    if entry.kind == spfs::tracking::EntryKind::Tree {
                                        trees.insert(format!("{dir}/{entry_name}"), &entry.entries);
                                    }
                                    self.print_file(entry, entry_name, &username, last_modified)
                                }
                            }
                            println!("\n");
                        }
                        entries_to_process = std::mem::take(&mut trees);
                    }
                }
            } else {
                for name in entries.keys().sorted() {
                    self.print_file(&entries[name], name, &username, last_modified);
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

    pub fn print_file(
        &mut self,
        entry: &spfs::tracking::Entry,
        file_name: &String,
        username: &String,
        last_modified: DateTime<Utc>,
    ) {
        if self.long {
            match entry.kind {
                spfs::tracking::EntryKind::Tree => {
                    println!(
                        "{} {} {} {} {}/",
                        unix_mode::to_string(entry.mode),
                        username,
                        entry.size,
                        last_modified.format("%b %e %Y"),
                        file_name
                    )
                }
                _ => println!(
                    "{} {} {} {} {}",
                    unix_mode::to_string(entry.mode),
                    username,
                    entry.size,
                    last_modified.format("%b %e %Y"),
                    file_name
                ),
            }
        } else if self.recursive {
            match entry.kind {
                spfs::tracking::EntryKind::Tree => {
                    print!("{}/  ", file_name)
                }
                _ => print!("{}  ", file_name),
            }
        } else {
            match entry.kind {
                spfs::tracking::EntryKind::Tree => {
                    println!("{}/", file_name)
                }
                _ => println!("{}", file_name),
            }
        }
    }
}
