use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdRead {
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

impl CmdRead {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<()> {
    }
}


def register(sub_parsers: argparse._SubParsersAction) -> None:

    read_cmd = sub_parsers.add_parser(
        "read", aliases=["read-file", "cat", "cat-file"], help=_read.__doc__
    )
    read_cmd.add_argument(
        "ref",
        metavar="REF",
        nargs=1,
        help="The tag or digest of the blob/payload to output",
    )
    read_cmd.add_argument(
        "path",
        metavar="PATH",
        nargs="?",
        help="If the given ref is not a blob, read the blob found at this path",
    )
    read_cmd.set_defaults(func=_read)


def _read(args: argparse.Namespace) -> None:
    """Output the contents of a stored payload to stdout."""

    config = spfs.get_config()
    repo = config.get_repository()
    item = repo.read_ref(args.ref[0])
    if isinstance(item, spfs.storage.Blob):
        blob = item
    elif not args.path:
        print(f"{Fore.RED}PATH must be given to read from {type(item).__name__} object")
        sys.exit(1)
    else:
        path = args.path
        if path.startswith("/spfs"):
            path = path[len("/spfs") :]
        manifest = spfs.compute_object_manifest(item, repo=repo)
        try:
            entry = manifest.get_path(path)
        except FileNotFoundError:
            print(f"File does not exist: {args.path}")
            sys.exit(1)
        if entry.kind is not spfs.tracking.EntryKind.BLOB:
            print(f"Path is a directory: {args.path}")
            sys.exit(1)
        blob = repo.read_blob(entry.object)

    with repo.payloads.open_payload(blob.digest()) as reader:
        shutil.copyfileobj(reader, sys.stdout.buffer)
