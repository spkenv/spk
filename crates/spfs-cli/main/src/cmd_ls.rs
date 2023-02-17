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
                    let mut new_entries: HashMap<String, HashMap<String, spfs::tracking::Entry>> =
                        HashMap::new();
                    let mut temp_new_entries: HashMap<
                        String,
                        HashMap<String, spfs::tracking::Entry>,
                    > = HashMap::new();
                    new_entries.insert(".".to_string(), entries);
                    loop {
                        if new_entries.is_empty() {
                            break;
                        }
                        for dir in new_entries.keys().sorted() {
                            println!("{dir}:");
                            for entry_name in new_entries[dir].keys().sorted() {
                                if let Some(entry) = new_entries[dir].get(entry_name) {
                                    match entry.kind {
                                        spfs::tracking::EntryKind::Tree => {
                                            print!("{entry_name}/  ");
                                            temp_new_entries.insert(
                                                format!("{dir}/{entry_name}"),
                                                entry.entries.clone(),
                                            );
                                        }
                                        _ => print!("{entry_name}  "),
                                    }
                                }
                            }
                            println!("\n");
                        }
                        new_entries = temp_new_entries.clone();
                        temp_new_entries.clear()
                    }
                }
            } else {
                for name in entries.keys().sorted() {
                    match entries[name].kind {
                        spfs::tracking::EntryKind::Tree => {
                            println!("{} {} {}/", entries[name].mode, entries[name].size, name)
                        }
                        _ => println!("{} {} {}", entries[name].mode, entries[name].size, name),
                    }
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
}
