import os
import argparse

import structlog

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument(
        "--pull",
        "-p",
        action="store_true",
        help="try to pull the latest iteration of each tag even if it exists locally",
    )
    run_cmd.add_argument(
        "ref",
        metavar="REF",
        nargs=1,
        help="The tag or id of the desired runtime, "
        "use '-' or an empty string to request an empty environment",
    )
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    config = spfs.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()
    runtime = runtimes.create_runtime()
    if args.ref and args.ref[0] not in ("-", ""):
        env_spec = spfs.tracking.EnvSpec(args.ref[0])
        for target in env_spec.tags:
            if args.pull or not repo.has_object(target):
                _logger.info("pulling target ref", ref=target)
                obj = spfs.pull_ref(target)
            else:
                obj = repo.read_object(target)

            runtime.push_digest(obj.digest)

    _logger.info("resolving entry process")
    cmd = spfs.build_command_for_runtime(runtime, args.cmd[0], *args.args)
    os.execv(cmd[0], cmd)
