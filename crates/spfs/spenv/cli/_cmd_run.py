import os
import argparse

import structlog

import spenv

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
        metavar="ENV",
        nargs=1,
        help="The environment spec of the desired runtime",
    )
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()
    runtime = runtimes.create_runtime()
    env_spec = spenv.tracking.EnvSpec(args.ref)
    if args.ref is not None:
        env_spec = spenv.tracking.EnvSpec(args.ref)
        for target in env_spec.tags:
            if args.pull or not repo.has_object(target):
                _logger.info("pulling target ref", ref=target)
                obj = spenv.pull_ref(target)
            else:
                obj = repo.read_object(target)

            runtime.push_digest(obj.digest)

    _logger.info("resolving entry process")
    cmd = spenv.build_command_for_runtime(runtime, args.cmd[0], *args.args)
    os.execv(cmd[0], cmd)
