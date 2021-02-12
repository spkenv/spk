import sys
import shutil
import argparse

from colorama import Fore, Style

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    ls_cmd = sub_parsers.add_parser(
        "ls", aliases=["list-dir", "list"], help=_ls.__doc__
    )
    ls_cmd.add_argument(
        "ref",
        metavar="REF",
        nargs=1,
        help="The tag or digest of the file tree to read from",
    )
    ls_cmd.add_argument(
        "path",
        metavar="PATH",
        nargs="?",
        default="/",
        help="The subdirectory to list, defaults to the root ('/spfs')",
    )
    ls_cmd.set_defaults(func=_ls)


def _ls(args: argparse.Namespace) -> None:
    """List the contents of a committed directory."""

    config = spfs.get_config()
    repo = config.get_repository()
    item = repo.read_ref(args.ref[0])

    path = args.path
    if path.startswith("/spfs"):
        path = path[len("/spfs") :]
    manifest = spfs.compute_object_manifest(item, repo=repo)
    try:
        entries = manifest.list_dir(path)
    except (FileNotFoundError, NotADirectoryError) as e:
        print(e)
        sys.exit(1)

    for name in entries:
        print(name)
