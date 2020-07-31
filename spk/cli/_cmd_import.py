import sys
from typing import Callable, Any
import os
import argparse
import spfs

import structlog
from colorama import Fore, Style

import spk
import spk.external


_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    import_cmd = sub_parsers.add_parser(
        "import", aliases=["i"], help=_import.__doc__, **parser_args,
    )

    spcomp2_group = import_cmd.add_argument_group()
    spcomp2_group.add_argument(
        "--spcomp2", action="store_true", help="Import the named package from spComp2"
    )
    spcomp2_group.add_argument(
        "--target-repo",
        "-r",
        type=str,
        metavar="NAME",
        default="origin",
        help="The repository to publish to. Any configured spfs repository can be named here.",
    )
    spcomp2_group.add_argument(
        "--publish", action="store_true", help="Also publish the packages after import"
    )
    spcomp2_group.add_argument(
        "--force",
        "-f",
        action="store_true",
        default=False,
        help="Forcefully overwrite any existing publishes",
    )
    spcomp2_group.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime (useful for speed and debugging)",
    )

    import_cmd.add_argument(
        "packages", metavar="FILE|NAME", nargs="+", help="The packages to import",
    )
    import_cmd.set_defaults(func=_import)
    return import_cmd


def _import(args: argparse.Namespace) -> None:
    """Import an external or previously exported package."""

    if args.spcomp2:
        _import_spcomp2s(args)

    else:
        for filename in args.packages:
            spk.import_package(filename)


def _import_spcomp2s(args: argparse.Namespace) -> None:

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)

    specs = []
    for name in args.packages:
        specs.extend(spk.external.import_spcomp2(name))

    print("\nThe following packages were imported:\n")
    for spec in specs:
        print(f"  {spk.io.format_ident(spec.pkg)} ", end="")
        print(spk.io.format_options(spec.build.resolve_all_options({})))
    print("")

    if args.publish is None:
        print("These packages are now available in the local repository")
        args.publish = bool(
            input("Do you want to also publish these packages? [y/N]: ").lower()
            in ("y", "yes")
        )

    if args.publish:
        publisher = spk.Publisher().with_target(args.target).force(args.force)
        for spec in specs:
            publisher.publish(spec.pkg)
