use structopt::StructOpt;

mod args;
// mod cmd_check;
// mod cmd_clean;
// mod cmd_commit;
// mod cmd_diff;
// mod cmd_edit;
// mod cmd_info;
// mod cmd_init;
// mod cmd_layers;
// mod cmd_log;
// mod cmd_ls;
// mod cmd_ls_tags;
// mod cmd_migrate;
// mod cmd_platforms;
// mod cmd_pull;
// mod cmd_push;
// mod cmd_read;
// mod cmd_reset;
mod cmd_run;
// mod cmd_runtimes;
// mod cmd_search;
// mod cmd_shell;
// mod cmd_tag;
// mod cmd_tags;
mod cmd_version;

use args::{Command, Opt};

#[tokio::main]
async fn main() {
    args::configure_sentry();

    let opt = args::Opt::from_args();
    match opt.verbose {
        0 => {
            if std::env::var("SPFS_DEBUG").is_ok() {
                std::env::set_var("RUST_LOG", "DEBUG");
            }
        }
        1 => std::env::set_var("RUST_LOG", "DEBUG"),
        _ => std::env::set_var("RUST_LOG", "TRACE"),
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
        Command::Run(mut cmd) => cmd.run(&config).await,
        Command::Shell(mut cmd) => cmd.run(&config).await,
        // Command::Edit(cmd) => cmd.run().await,
        // Command::Commit(cmd) => cmd.run().await,
        // Command::Reset(cmd) => cmd.run().await,
        // Command::Tag(cmd) => cmd.run().await,
        // Command::Push(cmd) => cmd.run().await,
        // Command::Pull(cmd) => cmd.run().await,
        // Command::Runtimes(cmd) => cmd.run().await,
        // Command::Layers(cmd) => cmd.run().await,
        // Command::Platforms(cmd) => cmd.run().await,
        // Command::Tags(cmd) => cmd.run().await,
        // Command::Info(cmd) => cmd.run().await,
        // Command::Log(cmd) => cmd.run().await,
        // Command::Search(cmd) => cmd.run().await,
        // Command::Diff(cmd) => cmd.run().await,
        // Command::LsTags(cmd) => cmd.run().await,
        // Command::Ls(cmd) => cmd.run().await,
        // Command::Migrate(cmd) => cmd.run().await,
        // Command::Check(cmd) => cmd.run().await,
        // Command::Clean(cmd) => cmd.run().await,
        // Command::Read(cmd) => cmd.run().await,
        // Command::Init(cmd) => cmd.run().await,
    };

    match result {
        Err(err) => {
            capture_if_relevant(&err);
            tracing::error!("{}", err);
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
