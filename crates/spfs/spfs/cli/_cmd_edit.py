import os
import argparse

import structlog

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    edit_cmd = sub_parsers.add_parser("edit", help=_edit.__doc__)
    edit_cmd.set_defaults(func=_edit)


def _edit(args: argparse.Namespace) -> None:
    """Make the current runtime editable."""

    spfs.make_active_runtime_editable()
    _logger.info("edit mode enabled")
