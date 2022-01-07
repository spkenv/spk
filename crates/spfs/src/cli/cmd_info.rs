// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::*;
use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdInfo {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Operate on a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        value_name = "REF",
        about = "Tag or reference to show information about"
    )]
    refs: Vec<String>,
}

impl CmdInfo {
    pub async fn run(&mut self, verbosity: usize, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote)?,
            None => config.get_repository()?.into(),
        };

        if self.refs.is_empty() {
            print_global_info(&repo)?;
        } else {
            for reference in self.refs.iter() {
                let item = repo.read_ref(reference.as_str())?;
                pretty_print_ref(item, &repo, verbosity)?;
            }
        }
        Ok(0)
    }
}

fn pretty_print_ref(
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
                spfs::io::format_digest(obj.digest()?.to_string(), Some(repo))?
            );
            println!("{}", "stack:".bright_blue());
            for reference in obj.stack {
                println!(
                    "  - {}",
                    spfs::io::format_digest(reference.to_string(), Some(repo))?
                );
            }
        }

        Object::Layer(obj) => {
            println!("{}", "layer:".green());
            println!(
                " {} {}",
                "refs:".bright_blue(),
                spfs::io::format_digest(obj.digest()?.to_string(), Some(repo))?
            );
            println!(
                " {} {}",
                "manifest:".bright_blue(),
                spfs::io::format_digest(obj.manifest.to_string(), Some(repo))?
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
                    spfs::io::format_digest(node.entry.object.to_string(), Some(repo))?,
                    node.path.to_string(),
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
                spfs::io::format_digest(obj.payload.to_string(), Some(repo))?
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
fn print_global_info(repo: &spfs::storage::RepositoryHandle) -> spfs::Result<()> {
    let runtime = spfs::active_runtime()?;

    println!("{}", "Active Runtime:".green());
    println!(
        " {}: {}",
        "id:".bright_blue(),
        runtime.reference().to_string_lossy()
    );
    println!(" {}: {}", "editable:".bright_blue(), runtime.is_editable());
    println!("{}", "stack".bright_blue());
    let stack = runtime.get_stack();
    for digest in stack {
        println!(
            "  - {}",
            spfs::io::format_digest(digest.to_string(), Some(repo))?
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
