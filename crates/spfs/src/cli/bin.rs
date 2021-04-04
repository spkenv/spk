use structopt::StructOpt;

mod args;
mod cmd_check;
mod cmd_clean;
mod cmd_commit;
mod cmd_diff;
mod cmd_edit;
mod cmd_info;
mod cmd_init;
mod cmd_join;
mod cmd_layers;
mod cmd_log;
mod cmd_ls;
mod cmd_ls_tags;
mod cmd_migrate;
mod cmd_platforms;
mod cmd_pull;
mod cmd_push;
mod cmd_read;
mod cmd_render;
mod cmd_reset;
mod cmd_runtimes;
mod cmd_search;
mod cmd_tag;
mod cmd_tags;
mod cmd_version;

use args::{Command, Opt};

fn main() {
    // because this function exits right away it does not
    // properly handle destruction of data, so we put the actual
    // logic into a separate function/scope
    std::process::exit(run())
}

fn run() -> i32 {
    let opt = args::Opt::from_args();
    args::configure_sentry();
    args::configure_logging(&opt);
    args::configure_spops(&opt);

    let config = match spfs::load_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return 1;
        }
        Ok(config) => config,
    };

    let result = match opt.cmd {
        Command::Version(cmd) => cmd.run(),
        Command::Edit(mut cmd) => cmd.run(&config),
        Command::Commit(mut cmd) => cmd.run(&config),
        Command::Reset(mut cmd) => cmd.run(&config),
        Command::Tag(mut cmd) => cmd.run(&config),
        Command::Push(mut cmd) => cmd.run(&config),
        Command::Pull(mut cmd) => cmd.run(&config),
        Command::Runtimes(mut cmd) => cmd.run(&config),
        Command::Join(mut cmd) => cmd.run(&config),
        Command::Layers(mut cmd) => cmd.run(&config),
        Command::Platforms(mut cmd) => cmd.run(&config),
        Command::Tags(mut cmd) => cmd.run(&config),
        Command::Info(mut cmd) => cmd.run(opt.verbose, &config),
        Command::Log(mut cmd) => cmd.run(&config),
        Command::Search(mut cmd) => cmd.run(&config),
        Command::Diff(mut cmd) => cmd.run(&config),
        Command::LsTags(mut cmd) => cmd.run(&config),
        Command::Ls(mut cmd) => cmd.run(&config),
        Command::Migrate(mut cmd) => cmd.run(&config),
        Command::Check(mut cmd) => cmd.run(&config),
        Command::Clean(mut cmd) => cmd.run(&config),
        Command::Read(mut cmd) => cmd.run(&config),
        Command::Render(mut cmd) => cmd.run(&config),
        Command::InitRuntime(mut cmd) => cmd.run(&config),
        Command::External(args) => run_external_subcommand(args),
    };

    match result {
        Err(err) => {
            capture_if_relevant(&err);
            tracing::error!("{}", spfs::io::format_error(&err));
            1
        }
        Ok(code) => code,
    }
}

fn run_external_subcommand(args: Vec<String>) -> spfs::Result<i32> {
    {
        let command = match args.get(0) {
            None => {
                tracing::error!("Invalid subcommand, cannot be empty");
                return Ok(1);
            }
            Some(c) => c,
        };
        let command = format!("spfs-{}", command);
        let command_cstr = match std::ffi::CString::new(command.clone()) {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("Invalid subcommand, not a valid string");
                return Ok(1);
            }
        };
        let mut args_cstr = Vec::with_capacity(args.len());
        args_cstr.push(command_cstr.clone());
        for arg in args.iter().skip(2) {
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

fn capture_if_relevant(err: &spfs::Error) {
    match err {
        spfs::Error::NoRuntime(_) => (),
        spfs::Error::UnknownObject(_) => (),
        spfs::Error::UnknownReference(_) => (),
        spfs::Error::AmbiguousReference(_) => (),
        spfs::Error::NothingToCommit(_) => (),
        _ => {
            sentry::capture_error(err);
        }
    }
}
