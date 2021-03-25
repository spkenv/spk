use structopt::StructOpt;

mod args;
mod cmd_check;
mod cmd_clean;
mod cmd_commit;
mod cmd_diff;
mod cmd_edit;
mod cmd_info;
mod cmd_init;
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
mod cmd_run;
mod cmd_runtimes;
mod cmd_search;
mod cmd_tag;
mod cmd_tags;
mod cmd_version;

use args::{Command, Opt};

fn main() {
    args::configure_sentry();

    let opt = args::Opt::from_args();
    match opt.verbose {
        0 => {
            if std::env::var("SPFS_DEBUG").is_ok() {
                std::env::set_var("RUST_LOG", "spfs=debug");
            } else if std::env::var("RUST_LOG").is_err() {
                std::env::set_var("RUST_LOG", "spfs=info");
            }
        }
        1 => std::env::set_var("RUST_LOG", "spfs=debug"),
        _ => std::env::set_var("RUST_LOG", "spfs=trace"),
    }

    args::configure_logging(&opt);
    args::configure_spops(&opt);

    sentry::configure_scope(|scope| {
        scope.set_extra("command", format!("{:?}", opt.cmd).into());
        scope.set_extra("argv", format!("{:?}", std::env::args()).into());
    });

    // TODO: spops collection
    // try:
    //     spops.count("spfs.run_count")
    //     with spops.timer("spfs.run_time"):
    //         args.func(args)

    // except KeyboardInterrupt:
    //     pass

    // except Exception as e:
    //     capture_if_relevant(e)
    //     _logger.error(str(e))
    //     spops.count("spfs.error_count")
    //     if args.debug:
    //         traceback.print_exc(file=sys.stderr)
    //     return 1

    // return 0

    let config = match spfs::load_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            std::process::exit(1);
        }
        Ok(config) => config,
    };

    let result = match opt.cmd {
        Command::Version(cmd) => cmd.run(),
        Command::Run(mut cmd) => cmd.run(&config),
        Command::Shell(mut cmd) => cmd.run(&config),
        Command::Edit(mut cmd) => cmd.run(&config),
        Command::Commit(mut cmd) => cmd.run(&config),
        Command::Reset(mut cmd) => cmd.run(&config),
        Command::Tag(mut cmd) => cmd.run(&config),
        Command::Push(mut cmd) => cmd.run(&config),
        Command::Pull(mut cmd) => cmd.run(&config),
        Command::Runtimes(mut cmd) => cmd.run(&config),
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
    };

    match result {
        Err(err) => {
            capture_if_relevant(&err);
            tracing::error!("{}", spfs::io::format_error(&err));
            std::process::exit(1);
        }
        Ok(_) => std::process::exit(0),
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
