use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdSearch {
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

impl CmdSearch {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
    }
}

def register(sub_parsers: argparse._SubParsersAction) -> None:

    tags_cmd = sub_parsers.add_parser("search", help=_search.__doc__)
    tags_cmd.add_argument("term", metavar="TERM", help="The search term / substring")
    tags_cmd.set_defaults(func=_search)


def _search(args: argparse.Namespace) -> None:
    """Search for available tags by substring."""

    config = spfs.get_config()
    repos = []
    for name in config.list_remote_names():
        try:
            repos.append(config.get_remote(name))
        except Exception as e:
            _LOGGER.warning("failed to open remote repository", remote=name)
            _LOGGER.warning("--> " + str(e))
    repos.insert(0, config.get_repository())
    for repo in repos:
        for spec, _ in repo.iter_tags():
            if args.term in spec:
                print(spec)
