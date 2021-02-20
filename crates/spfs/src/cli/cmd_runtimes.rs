use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdRuntimes {
    #[structopt(
        value_name = "FROM",
        about = "The tag or id to use as the base of the computed diff, defaults to the current runtime"
    )]
    base: Option<String>,
    #[structopt(
        value_name = "TO",
        about = "The tag or id to diff the base against, defaults to the contents of /spfs"
    )]
    top: Option<String>,
}

impl CmdRuntimes {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
    }
}

def register(sub_parsers: argparse._SubParsersAction) -> None:

    runtimes_cmd = sub_parsers.add_parser("runtimes", help=_runtimes.__doc__)
    runtimes_cmd.set_defaults(func=_runtimes)


def _runtimes(args: argparse.Namespace) -> None:
    """List the active set of spfs runtimes."""

    config = spfs.get_config()
    runtime_storage = config.get_runtime_storage()
    runtimes = runtime_storage.list_runtimes()
    for runtime in runtimes:
        print(runtime.ref)
