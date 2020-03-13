import sys
import argparse

import structlog
import spfs


_LOGGER = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    check_cmd = sub_parsers.add_parser("check", help=_check.__doc__)
    check_cmd.add_argument(
        "--remote", "-r", help=("Trigger the check operation on a remote repository"),
    )
    check_cmd.set_defaults(func=_check)


def _check(args: argparse.Namespace) -> None:
    """Check a repositories internal integrity."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    _LOGGER.info("walking repository...")
    try:
        spfs.graph.check_database_integrity(repo.objects)
    except Exception as e:
        _LOGGER.error(str(e))
        sys.exit(1)
    else:
        _LOGGER.info("repository OK")
