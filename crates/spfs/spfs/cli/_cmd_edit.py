import os
import argparse

import structlog

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    edit_cmd = sub_parsers.add_parser("edit", help=_edit.__doc__)
    edit_cmd.add_argument(
        "--off", action="store_true", default=False, help="Disable edit mode instead",
    )
    edit_cmd.set_defaults(func=_edit)


def _edit(args: argparse.Namespace) -> None:
    """Make the current runtime editable."""

    if not args.off:
        try:
            spfs.make_active_runtime_editable()
        except ValueError as err:
            _logger.info(str(err))
        else:
            _logger.info("edit mode enabled")
    else:
        rt = spfs.active_runtime()
        rt.set_editable(False)
        try:
            spfs.remount_runtime(rt)
        except:
            rt.set_editable(True)
            raise
