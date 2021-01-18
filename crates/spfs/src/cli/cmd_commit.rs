import sys
import argparse

import structlog

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    commit_cmd = sub_parsers.add_parser("commit", help=_commit.__doc__)
    commit_cmd.add_argument(
        "kind", choices=["layer", "platform"], help="The desired object type to create"
    )
    commit_cmd.add_argument(
        "--tag",
        "-t",
        dest="tags",
        action="append",
        help="Can be given many times: human-readable tags to update with the resulting object",
    )
    commit_cmd.set_defaults(func=_commit)


def _commit(args: argparse.Namespace) -> None:
    """Commit the current runtime state to storage."""

    runtime = spfs.active_runtime()

    if not runtime.is_editable():
        _logger.error("Active runtime is not editable, nothing to commmit")
        sys.exit(1)

    config = spfs.get_config()
    repo = config.get_repository()

    result: spfs.graph.Object
    if args.kind == "layer":
        result = spfs.commit_layer(runtime)
    elif args.kind == "platform":
        result = spfs.commit_platform(runtime)
    else:
        raise NotImplementedError("commit", args.kind)

    _logger.info("created", digest=result.digest())
    for tag in args or []:

        repo.push_tag(tag, result.digest())
        _logger.info("created", tag=tag)

    _logger.info("edit mode disabled")
    return
