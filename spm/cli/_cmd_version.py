from typing import List
import argparse

import spm


def register(sub_parsers: argparse._SubParsersAction) -> argparse.ArgumentParser:

    version_cmd = sub_parsers.add_parser("version", help=_version.__doc__)
    version_cmd.set_defaults(func=_version)
    return version_cmd


def _version(_: argparse.Namespace) -> None:
    """Print the spm version number and exit."""

    print(spm.__version__)
