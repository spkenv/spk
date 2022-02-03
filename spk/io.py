# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Iterable, Sequence, TextIO, Tuple, Union
from colorama import Fore, Style
import io
import sys

from spkrs.io import (
    format_ident,
    format_build,
    format_options,
    format_request,
    format_solution,
    format_note,
    format_change,
    format_decisions,
    change_is_relevant_at_verbosity
)
from . import api, solve


def run_and_print_resolve(
    solver: solve.Solver,
    verbosity: int = 1,
) -> solve.Solution:
    runtime = solver.run()
    format_decisions(runtime, out=sys.stdout)
    return runtime.solution()


def format_solve_graph(graph: solve.Graph, verbosity: int = 1) -> str:

    out = io.StringIO()
    format_decisions(graph.walk(), out, verbosity)
    return out.getvalue()


def format_error(err: Exception, verbosity: int = 0) -> str:

    msg = str(err)
    if isinstance(err, solve.SolverError):
        msg = "Failed to resolve"
        if verbosity == 0:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '--verbose/-v' for more info"
        elif verbosity < 2:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '-vv' for even more info"
        elif verbosity < 3:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '-vvv' for even more info"
    return f"{Fore.RED}{msg}{Style.RESET_ALL}"
