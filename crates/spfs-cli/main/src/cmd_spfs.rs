use clap::{Parser, Subcommand};
use spfs::{Error, OsErrorExt};
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;

use crate::{
    cmd_check,
    cmd_commit,
    cmd_config,
    cmd_diff,
    cmd_edit,
    cmd_info,
    cmd_init,
    cmd_layers,
    cmd_log,
    cmd_ls,
    cmd_ls_tags,
    cmd_migrate,
    cmd_platforms,
    cmd_pull,
    cmd_push,
    cmd_read,
    cmd_reset,
    cmd_run,
    cmd_runtime,
    cmd_search,
    cmd_shell,
    cmd_tag,
    cmd_tags,
    cmd_untag,
    cmd_version,
    cmd_write,
};

/// Filesystem isolation, capture and distribution.
#[derive(Debug, Parser)]
#[clap(
    about,
    after_help = "EXTERNAL SUBCOMMANDS:\
                  \n    render       render the contents of an environment or layer\
                  \n    monitor      watch a runtime and clean it up when complete\
                  "
)]
pub struct Opt {
    #[clap(flatten)]
    pub logging: cli::Logging,
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(strum::AsRefStr, Debug, Subcommand)]
#[strum(serialize_all = "lowercase")]
#[clap(trailing_var_arg = true, dont_delimit_trailing_values = true)]
pub enum Command {
    Version(cmd_version::CmdVersion),
    Init(cmd_init::CmdInit),
    Edit(cmd_edit::CmdEdit),
    Commit(cmd_commit::CmdCommit),
    Config(cmd_config::CmdConfig),
    Reset(cmd_reset::CmdReset),
    Run(cmd_run::CmdRun),
    Tag(cmd_tag::CmdTag),
    Untag(cmd_untag::CmdUntag),
    Shell(cmd_shell::CmdShell),
    Runtime(cmd_runtime::CmdRuntime),
    Layers(cmd_layers::CmdLayers),
    Platforms(cmd_platforms::CmdPlatforms),
    Tags(cmd_tags::CmdTags),
    Info(cmd_info::CmdInfo),
    Pull(cmd_pull::CmdPull),
    Push(cmd_push::CmdPush),
    Log(cmd_log::CmdLog),
    Search(cmd_search::CmdSearch),
    Diff(cmd_diff::CmdDiff),
    LsTags(cmd_ls_tags::CmdLsTags),
    Ls(cmd_ls::CmdLs),
    Migrate(cmd_migrate::CmdMigrate),
    Check(cmd_check::CmdCheck),
    Read(cmd_read::CmdRead),
    Write(cmd_write::CmdWrite),

    #[cfg(feature = "server")]
    Server(crate::cmd_server::CmdServer),

    #[clap(external_subcommand)]
    External(Vec<String>),
}

impl CommandName for Opt {
    fn command_name(&self) -> &str {
        self.cmd.as_ref()
    }
}

impl Opt {
    pub async fn run(&mut self, config: &spfs::Config) -> miette::Result<i32> {
        match &mut self.cmd {
            Command::Version(cmd) => cmd.run().await,
            Command::Edit(cmd) => cmd.run(config).await,
            Command::Init(cmd) => cmd.run(config).await,
            Command::Commit(cmd) => cmd.run(config).await,
            Command::Config(cmd) => cmd.run(config).await,
            Command::Reset(cmd) => cmd.run(config).await,
            Command::Tag(cmd) => cmd.run(config).await,
            Command::Untag(cmd) => cmd.run(config).await,
            Command::Runtime(cmd) => cmd.run(config).await,
            Command::Layers(cmd) => cmd.run(config).await,
            Command::Platforms(cmd) => cmd.run(config).await,
            Command::Tags(cmd) => cmd.run(config).await,
            Command::Info(cmd) => cmd.run(config).await,
            Command::Log(cmd) => cmd.run(config).await,
            Command::Search(cmd) => cmd.run(config).await,
            Command::Diff(cmd) => cmd.run(config).await,
            Command::LsTags(cmd) => cmd.run(config).await,
            Command::Ls(cmd) => cmd.run(config).await,
            Command::Migrate(cmd) => cmd.run(config).await,
            Command::Check(cmd) => cmd.run(config).await,
            Command::Read(cmd) => cmd.run(config).await,
            Command::Write(cmd) => cmd.run(config).await,
            Command::Run(cmd) => cmd.run(config).await,
            Command::Shell(cmd) => cmd.run(config).await,
            Command::Pull(cmd) => cmd.run(config).await,
            Command::Push(cmd) => cmd.run(config).await,
            #[cfg(feature = "server")]
            Command::Server(cmd) => cmd.run(config).await,
            Command::External(args) => run_external_subcommand(args.clone()).await,
        }
    }
}

async fn run_external_subcommand(args: Vec<String>) -> miette::Result<i32> {
    {
        let mut args = args.into_iter();
        let command = match args.next() {
            None => {
                tracing::error!("Invalid subcommand, cannot be empty");
                return Ok(1);
            }
            Some(c) => c,
        };

        // either in the PATH or next to the current binary
        let cmd_path = match spfs::which_spfs(&command) {
            Some(cmd) => cmd,
            None => {
                let mut p = std::env::current_exe()
                    .map_err(|err| Error::process_spawn_error("current_exe()", err, None))?;
                p.set_file_name(&command);
                p
            }
        };

        let cmd = spfs::bootstrap::Command {
            executable: cmd_path.into(),
            args: args.map(Into::into).collect(),
            vars: Vec::new(),
        };

        match cmd.exec() {
            Ok(o) => match o {},
            Err(err) if err.is_os_not_found() => {
                tracing::error!("{command} not found in PATH, was it properly installed?")
            }
            Err(err) => tracing::error!("subcommand failed: {err:?}"),
        }
        Ok(1)
    }
}
