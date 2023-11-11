// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use colored::*;
use miette::Result;
use spfs::env::SPFS_DIR;
use spfs::find_path::ObjectPathEntry;
use spfs::graph::Object;
use spfs::io::{self, DigestFormat, Pluralize};
use spfs::prelude::*;
use spfs::{self};
use spfs_cli_common as cli;

/// Display information about the current environment, or specific items
#[derive(Debug, Args)]
pub struct CmdInfo {
    #[clap(flatten)]
    logging: cli::Logging,

    /// Lists file sizes in human readable format
    #[clap(long, short = 'H')]
    human_readable: bool,

    /// Operate on a remote repository instead of the local one
    ///
    /// This is really only helpful if you are providing a specific ref to look up.
    #[clap(long, short)]
    remote: Option<String>,

    /// Tag, id, or /spfs/file/path to show information about
    #[clap(value_name = "REF")]
    refs: Vec<String>,

    /// Also find and report any tags that point to any identified digest (implies '--short')
    #[clap(long)]
    tags: bool,

    /// Use shortened digests in the output (nicer, but slower)
    #[clap(long)]
    short: bool,
}

impl CmdInfo {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        if self.refs.is_empty() {
            self.print_global_info(&repo).await?;
        } else {
            let number_refs = self.refs.len();
            for (index, reference) in self.refs.iter().enumerate() {
                if reference.starts_with(spfs::env::SPFS_DIR) {
                    self.pretty_print_file(
                        reference,
                        &repo,
                        self.logging.verbose as usize,
                        number_refs,
                    )
                    .await?;
                } else {
                    let item = repo.read_ref(reference.as_str()).await?;
                    self.pretty_print_ref(item, &repo, self.logging.verbose as usize)
                        .await?;
                }
                if index + 1 < number_refs {
                    println!();
                }
            }
        }
        Ok(0)
    }

    // TODO: there are 2 other versions of this in cmd_layers and
    // cmd_platforms, might be possible to combine them
    /// Return a String based on the given digest and the --tags argument
    async fn format_digest<'repo>(
        &self,
        digest: spfs::encoding::Digest,
        repo: &'repo spfs::storage::RepositoryHandle,
    ) -> Result<String> {
        if self.tags {
            io::format_digest(digest, DigestFormat::ShortenedWithTags(repo)).await
        } else if self.short {
            io::format_digest(digest, DigestFormat::Shortened(repo)).await
        } else {
            io::format_digest(digest, DigestFormat::Full).await
        }
        .map_err(|err| err.into())
    }

    /// Display the spfs object locations that provide the given file
    async fn pretty_print_ref(
        &self,
        obj: spfs::graph::Object,
        repo: &spfs::storage::RepositoryHandle,
        verbosity: usize,
    ) -> Result<()> {
        match obj {
            Object::Platform(obj) => {
                println!(
                    "{}:\n{}",
                    self.format_digest(obj.digest()?, repo).await?,
                    "platform:".green()
                );
                println!(
                    " {} {}",
                    "refs:".bright_blue(),
                    self.format_digest(obj.digest()?, repo).await?
                );
                println!("{}", "stack:".bright_blue());
                for reference in obj.stack {
                    println!("  - {}", self.format_digest(reference, repo).await?);
                }
            }

            Object::Layer(obj) => {
                println!(
                    "{}:\n{}",
                    self.format_digest(obj.digest()?, repo).await?,
                    "layer:".green()
                );
                println!(
                    " {} {}",
                    "refs:".bright_blue(),
                    self.format_digest(obj.digest()?, repo).await?
                );
                println!(
                    " {} {}",
                    "manifest:".bright_blue(),
                    self.format_digest(obj.manifest, repo).await?
                );
            }

            Object::Manifest(obj) => {
                println!(
                    "{}:\n{}",
                    self.format_digest(obj.digest()?, repo).await?,
                    "manifest:".green()
                );
                let max_entries = match verbosity {
                    0 => 10,
                    1 => 50,
                    _ => usize::MAX,
                };
                let mut count = 0;
                for node in obj.to_tracking_manifest().walk_abs(SPFS_DIR) {
                    println!(
                        " {:06o} {} {} {}",
                        node.entry.mode,
                        node.entry.kind,
                        self.format_digest(node.entry.object, repo).await?,
                        node.path,
                    );
                    count += 1;
                    if count >= max_entries {
                        println!("{}", "   ...[truncated] use -vv for more".dimmed());
                        break;
                    }
                }
            }

            Object::Blob(obj) => {
                println!(
                    "{}:\n {}:",
                    self.format_digest(obj.payload, repo).await?,
                    "blob".green()
                );
                println!(
                    " {} {}",
                    "digest:".bright_blue(),
                    self.format_digest(obj.payload, repo).await?
                );
                println!(
                    " {} {}",
                    "size:".bright_blue(),
                    spfs::io::format_size(obj.size)
                );
            }
            Object::Tree(_) | Object::Mask => println!("{obj:?}"),
        }
        Ok(())
    }

    /// Display the status of the current runtime.
    async fn print_global_info(&self, repo: &spfs::storage::RepositoryHandle) -> Result<()> {
        let runtime = spfs::active_runtime().await?;

        println!("{}", "Active Runtime:".green());
        println!(" {}: {}", "id:".bright_blue(), runtime.name());
        println!(
            " {}: {}",
            "editable:".bright_blue(),
            runtime.status.editable
        );
        println!("{}", "stack".bright_blue());
        for digest in runtime.status.stack.iter() {
            print!("  - {}, ", self.format_digest(*digest, repo).await?);
            let object = repo.read_ref(digest.to_string().as_str()).await?;
            println!(
                "Size: {}",
                self.human_readable(object.calculate_object_size(repo).await?),
            );
        }
        println!();

        if !runtime.is_dirty() {
            println!("{}", "No Active Changes".red());
        } else {
            println!("{}", "Run 'spfs diff' for active changes".bright_blue());
        }
        Ok(())
    }

    /// Displays human readable size
    fn human_readable(&self, size: u64) -> String {
        if self.human_readable {
            spfs::io::format_size(size)
        } else {
            size.to_string()
        }
    }

    /// Display the spfs object locations that provide the given file
    async fn pretty_print_file(
        &self,
        filepath: &str,
        repo: &spfs::storage::RepositoryHandle,
        verbosity: usize,
        number_refs: usize,
    ) -> Result<()> {
        let mut in_a_runtime = true;
        let found = match spfs::find_path::find_path_providers_in_spfs_runtime(filepath, repo).await
        {
            Ok(f) => f,
            Err(spfs::Error::NoActiveRuntime) => {
                in_a_runtime = false;
                Vec::new()
            }
            Err(err) => return Err(err.into()),
        };

        if found.is_empty() {
            println!("{filepath}: {}", "not found".yellow());
            println!(
                " - {}",
                if in_a_runtime {
                    "not found in current /spfs runtime".yellow()
                } else {
                    "No active runtime".red()
                }
            );
        } else {
            if let Some(first_path) = found.first() {
                if let Some(ObjectPathEntry::FilePath(file_entry)) = first_path.last() {
                    let number = found.len();
                    let query = if number_refs > 1 {
                        format!("{filepath}: ")
                    } else {
                        "".to_string()
                    };
                    println!(
                        "{}{} (in {} {}{})",
                        query,
                        file_entry.kind.to_string().green(),
                        number,
                        "layer".pluralize(number),
                        if verbosity < 1 && number > 1 {
                            ", topmost 1 shown, use -v to see all"
                        } else {
                            ""
                        }
                    );
                }
            }

            // TODO: this logic is the same as self.format_digest() has and
            // the copies in cmd_layer and cmd_platform
            let digest_format = if self.tags {
                DigestFormat::ShortenedWithTags(repo)
            } else if self.short {
                DigestFormat::Shortened(repo)
            } else {
                DigestFormat::Full
            };
            io::pretty_print_filepaths(filepath, found, verbosity, digest_format).await?;
        }

        Ok(())
    }
}
