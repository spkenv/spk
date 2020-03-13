import os
import argparse

import structlog

import spfs

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
        metavar="REF",
        nargs="?",
        help="The tag or id of the desired runtime, "
        "use '-' or nothing to request an empty environment",
    )
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:
    """Enter a subshell in a configured spfs environment."""

    config = spfs.get_config()
    repo = config.get_repository()
    runtimes = config.get_runtime_storage()
    runtime = runtimes.create_runtime()
    if args.ref and args.ref != "-":
        env_spec = spfs.tracking.EnvSpec(args.ref)
        for target in env_spec.tags:
            if args.pull or not repo.has_ref(target):
                _logger.info("pulling target ref", ref=target)
                obj = spfs.pull_ref(target)
            else:
                obj = repo.read_ref(target)

            runtime.push_digest(obj.digest())

    _logger.info("resolving entry process")
    cmd = spfs.build_command_for_runtime(runtime, "")
    _logger.debug(" ".join(cmd))
    os.execv(cmd[0], cmd)
