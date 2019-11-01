import argparse

import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    commit_cmd = sub_parsers.add_parser("commit", help=_commit.__doc__)
    commit_cmd.add_argument("kind", choices=["layer", "platform"], help="TODO: help")
    commit_cmd.add_argument(
        "--tag", "-t", dest="tags", action="append", help="TODO: help"
    )
    commit_cmd.set_defaults(func=_commit)


def _commit(args: argparse.Namespace) -> None:
    """Commit the current runtime state to storage."""

    runtime = spenv.active_runtime()
    config = spenv.get_config()
    repo = config.get_repository()

    result: spenv.storage.Object
    if args.kind == "layer":
        result = spenv.commit_layer(runtime)
    elif args.kind == "platform":
        result = spenv.commit_platform(runtime)
    else:
        raise NotImplementedError("commit", args.kind)

    _logger.info("created", digest=result.digest)
    for tag in args.tags or []:

        repo.push_tag(tag, result.digest)
        _logger.info("created", tag=tag)

    return
