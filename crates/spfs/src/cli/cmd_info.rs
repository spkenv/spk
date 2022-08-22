// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use colored::*;

use spfs::io::{self, DigestFormat};
use spfs::{self, prelude::*};

/// Display information about the current environment, or specific items
#[derive(Debug, Args)]
pub struct CmdInfo {
    /// Operate on a remote repository instead of the local one
    ///
    /// This is really only helpful if you are providing a specific ref to look up.
    #[clap(long, short)]
    remote: Option<String>,

    /// Tag or id to show information about
    #[clap(value_name = "REF")]
    refs: Vec<String>,

    /// Also find and report any tags that point to any identified digest
    #[clap(long)]
    tags: bool,
}

impl CmdInfo {
    pub async fn run(&mut self, verbosity: usize, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        if self.refs.is_empty() {
            self.print_global_info(&repo).await?;
        } else {
            for reference in self.refs.iter() {
                let item = repo.read_ref(reference.as_str()).await?;
                self.pretty_print_ref(item, &repo, verbosity).await?;
            }
        }
        Ok(0)
    }

    async fn format_digest<'repo>(
        &self,
        digest: spfs::encoding::Digest,
        repo: &'repo spfs::storage::RepositoryHandle,
    ) -> spfs::Result<String> {
        if self.tags {
            io::format_digest(digest, DigestFormat::ShortenedWithTags(repo)).await
        } else {
            io::format_digest(digest, DigestFormat::Shortened(repo)).await
        }
    }

    async fn pretty_print_ref(
        &self,
        obj: spfs::graph::Object,
        repo: &spfs::storage::RepositoryHandle,
        verbosity: usize,
    ) -> spfs::Result<()> {
        use spfs::graph::Object;
        match obj {
            Object::Platform(obj) => {
                println!("{}", "platform:".green());
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
                println!("{}", "layer:".green());
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
                println!("{}", "manifest:".green());
                let max_entries = match verbosity {
                    0 => 10,
                    1 => 50,
                    _ => usize::MAX,
                };
                let mut count = 0;
                for node in obj.unlock().walk_abs("/spfs") {
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
                println!("{}", "blob:".green());
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
            Object::Tree(_) | Object::Mask => println!("{:?}", obj),
        }
        Ok(())
    }

    /// Display the status of the current runtime.
    async fn print_global_info(&self, repo: &spfs::storage::RepositoryHandle) -> spfs::Result<()> {
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
            println!("  - {}", self.format_digest(*digest, repo).await?);
        }
        println!();

        if !runtime.is_dirty() {
            println!("{}", "No Active Changes".red());
        } else {
            println!("{}", "Run 'spfs diff' for active changes".bright_blue());
        }
        Ok(())
    }
}
