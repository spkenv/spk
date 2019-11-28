import os
import argparse

import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.add_argument(
        "--pull",
        "-p",
        action="store_true",
        help="try to pull the latest iteration of each tag even if it exists locally",
    )
    shell_cmd.add_argument(
        "ref",
        metavar="ENV",
        nargs="?",
        help="The environment spec of the desired runtime",
    )
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    config = spenv.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()
    runtime = runtimes.create_runtime()
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
    cmd = spenv.build_command_for_runtime(runtime, "")
    _logger.debug(" ".join(cmd))
    os.execv(cmd[0], cmd)
