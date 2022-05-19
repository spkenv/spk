// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::{Parser, Subcommand};

#[macro_use]
mod args;
mod cmd_check;
mod cmd_clean;
mod cmd_commit;
mod cmd_config;
mod cmd_diff;
mod cmd_edit;
mod cmd_info;
mod cmd_layers;
mod cmd_log;
mod cmd_ls;
mod cmd_ls_tags;
mod cmd_migrate;
mod cmd_monitor;
mod cmd_platforms;
mod cmd_read;
mod cmd_reset;
mod cmd_runtimes;
mod cmd_search;
mod cmd_tag;
mod cmd_tags;
mod cmd_untag;
mod cmd_version;
mod cmd_write;

main!(Opt);

/// Filesystem isolation, capture and distribution.
#[derive(Debug, Parser)]
#[clap(
    about,
    after_help = "EXTERNAL SUBCOMMANDS:\
                  \n    run          run a command in an spfs environment\
                  \n    shell        create a new shell in an spfs environment\
                  \n    pull         pull one or more object to the local repository\
                  \n    push         push one or more objects to a remote repository\
                  \n    render       render the contents of an environment or layer\
                  \n    server       run an spfs server (if installed)\
                  "
)]
pub struct Opt {
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: usize,
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, Subcommand)]
#[clap(trailing_var_arg = true, dont_delimit_trailing_values = true)]
pub enum Command {
    Version(cmd_version::CmdVersion),
    Edit(cmd_edit::CmdEdit),
    Commit(cmd_commit::CmdCommit),
    Config(cmd_config::CmdConfig),
    Reset(cmd_reset::CmdReset),
    Tag(cmd_tag::CmdTag),
    Untag(cmd_untag::CmdUntag),
    Runtimes(cmd_runtimes::CmdRuntimes),
    Layers(cmd_layers::CmdLayers),
    Platforms(cmd_platforms::CmdPlatforms),
    Tags(cmd_tags::CmdTags),
    Info(cmd_info::CmdInfo),
    Log(cmd_log::CmdLog),
    Search(cmd_search::CmdSearch),
    Diff(cmd_diff::CmdDiff),
    LsTags(cmd_ls_tags::CmdLsTags),
    Ls(cmd_ls::CmdLs),
    Migrate(cmd_migrate::CmdMigrate),
    Monitor(cmd_monitor::CmdMonitor),
    Check(cmd_check::CmdCheck),
    Clean(cmd_clean::CmdClean),
    Read(cmd_read::CmdRead),
    Write(cmd_write::CmdWrite),

    #[clap(external_subcommand)]
    External(Vec<String>),
}

impl Opt {
    async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        match &mut self.cmd {
            Command::Version(cmd) => cmd.run().await,
            Command::Edit(cmd) => cmd.run(config).await,
            Command::Commit(cmd) => cmd.run(config).await,
            Command::Config(cmd) => cmd.run(config).await,
            Command::Reset(cmd) => cmd.run(config).await,
            Command::Tag(cmd) => cmd.run(config).await,
            Command::Untag(cmd) => cmd.run(config).await,
            Command::Runtimes(cmd) => cmd.run(config).await,
            Command::Layers(cmd) => cmd.run(config).await,
            Command::Platforms(cmd) => cmd.run(config).await,
            Command::Tags(cmd) => cmd.run(config).await,
            Command::Info(cmd) => cmd.run(self.verbose, config).await,
            Command::Log(cmd) => cmd.run(config).await,
            Command::Search(cmd) => cmd.run(config).await,
            Command::Diff(cmd) => cmd.run(config).await,
            Command::LsTags(cmd) => cmd.run(config).await,
            Command::Ls(cmd) => cmd.run(config).await,
            Command::Migrate(cmd) => cmd.run(config).await,
            Command::Monitor(cmd) => cmd.run(config).await,
            Command::Check(cmd) => cmd.run(config).await,
            Command::Clean(cmd) => cmd.run(config).await,
            Command::Read(cmd) => cmd.run(config).await,
            Command::Write(cmd) => cmd.run(config).await,
            Command::External(args) => run_external_subcommand(args.clone()).await,
        }
    }
}

async fn run_external_subcommand(args: Vec<String>) -> spfs::Result<i32> {
    {
        let command = match args.get(0) {
            None => {
                tracing::error!("Invalid subcommand, cannot be empty");
                return Ok(1);
            }
            Some(c) => c,
        };

        // either in the PATH or next to the current binary
        let command = format!("spfs-{command}");
        let cmd_path = match spfs::which(command.as_str()) {
            Some(cmd) => cmd,
            None => {
                let mut p = std::env::current_exe()?;
                p.set_file_name(&command);
                p
            }
        };
        let command_cstr = match std::ffi::CString::new(cmd_path.to_string_lossy().to_string()) {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("Invalid subcommand, not a valid string");
                return Ok(1);
            }
        };
        let mut args_cstr = Vec::with_capacity(args.len());
        args_cstr.push(command_cstr.clone());
        for arg in args.iter().skip(1) {
            args_cstr.push(match std::ffi::CString::new(arg.clone()) {
                Ok(s) => s,
                Err(_) => {
                    tracing::error!("Invalid argument, not a valid string");
                    return Ok(1);
                }
            })
        }
        if let Err(err) = nix::unistd::execvp(command_cstr.as_c_str(), args_cstr.as_slice()) {
            match err.as_errno() {
                Some(nix::errno::Errno::ENOENT) => {
                    tracing::error!("{command} not found in PATH, was it properly installed?")
                }
                _ => tracing::error!("subcommand failed: {err:?}"),
            }
            return Ok(1);
        }
        Ok(0)
    }
}
