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
    print_decisions,
    format_error,
    change_is_relevant_at_verbosity,
)
from . import api, solve


def run_and_print_resolve(
    solver: solve.Solver,
    verbosity: int = 1,
) -> solve.Solution:
    runtime = solver.run()
    print_decisions(runtime)
    return runtime.solution()


def format_solve_graph(graph: solve.Graph, verbosity: int = 1) -> str:

    return format_decisions(graph.walk(), verbosity)
