// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

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
mod cmd_platforms;
mod cmd_read;
mod cmd_reset;
mod cmd_runtimes;
mod cmd_search;
mod cmd_tag;
mod cmd_tags;
mod cmd_untag;
mod cmd_version;

main!(Opt);

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs",
    version = spfs::VERSION,
    about = "Filesystem isolation, capture and distribution.",
    after_help = "EXTERNAL SUBCOMMANDS:\
                  \n    init         run a command in the current spfs shell environment\
                  \n    run          run a command in an spfs environment\
                  \n    shell        create a new shell in an spfs environment\
                  \n    pull         pull one or more object to the local repository\
                  \n    push         push one or more objects to a remote repository\
                  \n    render       render the contents of an environment or layer\
                  \n    server       run an spfs server (if installed)\
                  "
)]
pub struct Opt {
    #[structopt(short = "v", long = "verbose", global = true, parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(short = "h", long = "help", about = "Prints help information")]
    pub help: bool,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::DontDelimitTrailingValues)]
#[structopt(setting = structopt::clap::AppSettings::TrailingVarArg)]
pub enum Command {
    #[structopt(about = "print the version of spfs")]
    Version(cmd_version::CmdVersion),
    #[structopt(about = "make the current runtime editable")]
    Edit(cmd_edit::CmdEdit),
    #[structopt(about = "commit the current runtime state to storage")]
    Commit(cmd_commit::CmdCommit),
    #[structopt(about = "output configuration information")]
    Config(cmd_config::CmdConfig),
    #[structopt(about = "rebuild /spfs with the requested refs, removing any active changes")]
    Reset(cmd_reset::CmdReset),
    #[structopt(about = "tag an object")]
    Tag(cmd_tag::CmdTag),
    #[structopt(about = "remove tag versions or entire tag streams")]
    Untag(cmd_untag::CmdUntag),
    #[structopt(about = "list the current set of spfs runtimes")]
    Runtimes(cmd_runtimes::CmdRuntimes),
    #[structopt(about = "list all layers in an spfs repository")]
    Layers(cmd_layers::CmdLayers),
    #[structopt(about = "list all platforms in an spfs repository")]
    Platforms(cmd_platforms::CmdPlatforms),
    #[structopt(about = "list all tags in an spfs repository")]
    Tags(cmd_tags::CmdTags),
    #[structopt(about = "display information about the current environment or specific items")]
    Info(cmd_info::CmdInfo),
    #[structopt(about = "log the history of a given tag over time")]
    Log(cmd_log::CmdLog),
    #[structopt(about = "search for available tags by substring")]
    Search(cmd_search::CmdSearch),
    #[structopt(about = "compare two spfs file system states")]
    Diff(cmd_diff::CmdDiff),
    #[structopt(about = "list tags by their path", visible_aliases = &["list-tags"])]
    LsTags(cmd_ls_tags::CmdLsTags),
    #[structopt(about = "list the contents of a committed directory", visible_aliases = &["list-dir", "list"])]
    Ls(cmd_ls::CmdLs),
    #[structopt(about = "migrate the data from and older repository format to the latest one")]
    Migrate(cmd_migrate::CmdMigrate),
    #[structopt(about = "check a repositories internal integrity")]
    Check(cmd_check::CmdCheck),
    #[structopt(about = "clean the repository storage of untracked data")]
    Clean(cmd_clean::CmdClean),
    #[structopt(about = "output the contents of a stored payload to stdout", visible_aliases = &["read-file", "cat", "cat-file"])]
    Read(cmd_read::CmdRead),

    #[structopt(external_subcommand)]
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
            Command::Check(cmd) => cmd.run(config).await,
            Command::Clean(cmd) => cmd.run(config).await,
            Command::Read(cmd) => cmd.run(config).await,
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
        let command = format!("spfs-{}", command);
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
                    tracing::error!("{} not found in PATH, was it properly installed?", command)
                }
                _ => tracing::error!("subcommand failed: {:?}", err),
            }
            return Ok(1);
        }
        Ok(0)
    }
}
