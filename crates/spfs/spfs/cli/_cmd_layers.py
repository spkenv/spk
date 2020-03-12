import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    layers_cmd = sub_parsers.add_parser("layers", help=_layers.__doc__)
    layers_cmd.add_argument(
        "--remote",
        "-r",
        help="Show layers from remote repository instead of the local one",
    )
    layers_cmd.set_defaults(func=_layers)


def _layers(args: argparse.Namespace) -> None:

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()
    for layer in repo.iter_layers():
        print(spfs.io.format_digest(layer.digest()))
