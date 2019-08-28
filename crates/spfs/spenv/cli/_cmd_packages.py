import argparse

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    packages_cmd = sub_parsers.add_parser("packages", help=_packages.__doc__)
    packages_cmd.set_defaults(func=_packages)


def _packages(args: argparse.Namespace) -> None:

    config = spenv.get_config()
    repo = config.get_repository()
    packages = repo.packages.list_packages()
    for package in packages:
        print(package.ref)
