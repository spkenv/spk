import argparse

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    runtimes_cmd = sub_parsers.add_parser("runtimes", help=_runtimes.__doc__)
    runtimes_cmd.set_defaults(func=_runtimes)


def _runtimes(args: argparse.Namespace) -> None:

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = repo.runtimes.list_runtimes()
    for runtime in runtimes:
        print(runtime.ref)
