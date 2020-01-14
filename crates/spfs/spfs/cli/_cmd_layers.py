import argparse

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    layers_cmd = sub_parsers.add_parser("layers", help=_layers.__doc__)
    layers_cmd.set_defaults(func=_layers)


def _layers(args: argparse.Namespace) -> None:

    config = spfs.get_config()
    repo = config.get_repository()
    layers = repo.layers.list_layers()
    for layer in layers:
        print(spfs.io.format_digest(layer.digest))
