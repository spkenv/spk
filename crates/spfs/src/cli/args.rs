use sentry::IntoDsn;
use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spfs",
    about = "Filesystem isolation, capture and distribution."
)]
pub struct Opt {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(about = "print the version of dst")]
    Version(super::cmd_version::CmdVersion),
    #[structopt(about = "run a program in a configured environment")]
    Run(super::cmd_run::CmdRun),
    #[structopt(about = "enter a subshell in a configured spfs environment")]
    Shell(super::cmd_run::CmdShell),
    // #[structopt(about = "make the current runtime editable")]
    // Edit(super::cmd_edit::CmdEdit),
    // #[structopt(about = "commit the current runtime state to storage")]
    // Commit(super::cmd_commit::CmdCommit),
    // #[structopt(
    //     about = "rebuild the current /spfs dir with the requested refs, removing any active changes"
    // )]
    // Reset(super::cmd_reset::CmdReset),
    // #[structopt(about = "tag and object")]
    // Tag(super::cmd_tag::CmdTag),
    // #[structopt(about = "push one or more objects to a remote repository")]
    // Push(super::cmd_push::CmdPush),
    // #[structopt(about = "pull one or more objects to the local repository")]
    // Pull(super::cmd_pull::CmdPull),
    // #[structopt(about = "list the current set of spfs runtimes")]
    // Runtimes(super::cmd_runtimes::CmdRuntimes),
    // #[structopt(about = "list all layers in an spfs repository")]
    // Layers(super::cmd_layers::CmdLayers),
    // #[structopt(about = "list all platforms in an spfs repository")]
    // Platforms(super::cmd_platforms::CmdPlatforms),
    // #[structopt(about = "list all tags in an spfs repository")]
    // Tags(super::cmd_tags::CmdTags),
    // #[structopt(about = "display information about the current environment or specific items")]
    // Info(super::cmd_info::CmdInfo),
    // #[structopt(about = "log the history of a given tag over time")]
    // Log(super::cmd_log::CmdLog),
    // #[structopt(about = "search for available tags by substring")]
    // Search(super::cmd_search::CmdSearch),
    // #[structopt(about = "compare two spfs file system states")]
    // Diff(super::cmd_diff::CmdDiff),
    // #[structopt(about = "list tags by their path", aliases = ["list-tags"])]
    // LsTags(super::cmd_ls_tags::Ls_tagsCmd),
    // #[structopt(about = "list the contents of a committed directory", aliases = ["list-dir", "list"])]
    // Ls(super::cmd_ls::CmdLs),
    // #[structopt(about = "migrate the data from and older repository format to the latest one")]
    // Migrate(super::cmd_migrate::CmdMigrate),
    // #[structopt(about = "check a repositories internal integrity")]
    // Check(super::cmd_check::CmdCheck),
    // #[structopt(about = "clean the repository storage of untracked data")]
    // Clean(super::cmd_clean::CmdClean),
    // #[structopt(about = "output the contents of a stored payload to stdout", aliases = ["read-file", "cat", "cat-file"])]
    // Read(super::cmd_read::CmdRead),
    // #[structopt(about = "[internal use only] instantiates a raw runtime session")]
    // Init(super::cmd_init::CmdInit),
}

pub fn configure_sentry() {
    let mut opts = sentry::ClientOptions {
        dsn: "http://3dd72e3b4b9a4032947304fabf29966e@sentry.k8s.spimageworks.com/4"
            .into_dsn()
            .unwrap_or(None),
        environment: Some(
            std::env::var("SENTRY_ENVIRONMENT")
                .unwrap_or("production".to_string())
                .into(),
        ),
        // spdev follows sentry recommendation of using the release
        // tag as the name of the release in sentry
        release: Some(format!("v{}", spfs::VERSION).into()),
        ..Default::default()
    };
    opts = sentry::apply_defaults(opts);
    let _guard = sentry::init(opts);

    sentry::configure_scope(|scope| {
        let username = whoami::username();
        scope.set_user(Some(sentry::protocol::User {
            email: Some(format!("{}@imageworks.com", &username)),
            username: Some(username),
            ..Default::default()
        }))
    })
}

pub fn configure_spops(_: &Opt) {
    // TODO: have something for this
    // try:
    //     spops.configure(
    //         {
    //             "statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111},
    //             "labels": {
    //                 "environment": os.getenv("SENTRY_ENVIRONMENT", "production"),
    //                 "user": getpass.getuser(),
    //                 "host": socket.gethostname(),
    //             },
    //         },
    //     )
    // except Exception as e:
    //     print(f"failed to initialize spops: {e}", file=sys.stderr)
}

pub fn configure_logging(_opt: &super::Opt) {
    use tracing_subscriber::layer::SubscriberExt;
    let filter = tracing_subscriber::filter::EnvFilter::from_default_env();
    let registry = tracing_subscriber::Registry::default().with(filter);
    let fmt_layer = tracing_subscriber::fmt::layer().without_time();
    let sub = registry.with(fmt_layer);
    tracing::subscriber::set_global_default(sub).unwrap();
}
