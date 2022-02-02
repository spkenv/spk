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


def format_decisions(
    decisions: Iterable[Tuple[solve.graph.Node, solve.graph.Decision]],
    out: TextIO,
    verbosity: int = 1,
) -> None:
    level = 0
    for _, decision in decisions:
        if verbosity > 1:
            for note in decision.iter_notes():
                out.write(f"{'.'*level} {format_note(note)}\n")

        level_change = 1
        for change in decision.iter_changes():

            if isinstance(change, solve.graph.SetPackage):
                if change.spec.pkg.build == api.EMBEDDED:
                    fill = "."
                else:
                    fill = ">"
            elif isinstance(change, solve.graph.StepBack):
                fill = "!"
                level_change = -1
            else:
                fill = "."

            if not change_is_relevant_at_verbosity(change, verbosity):
                continue

            out.write(f"{fill*level} {format_change(change, verbosity)}\n")
        level += level_change


def change_is_relevant_at_verbosity(change: solve.graph.Change, verbosity: int) -> bool:

    levels = {
        solve.graph.SetPackage: 1,
        solve.graph.StepBack: 1,
        solve.graph.RequestPackage: 2,
        solve.graph.RequestVar: 2,
        solve.graph.SetOptions: 3,
    }

    for kind, level in levels.items():
        if isinstance(change, kind):
            return bool(verbosity >= level)
    return bool(verbosity >= 2)


def format_change(change: solve.graph.Change, _verbosity: int = 1) -> str:

    if isinstance(change, solve.graph.RequestPackage):
        return f"{Fore.BLUE}REQUEST{Fore.RESET} {format_request(change.request.pkg.name, [change.request])}"
    elif isinstance(change, solve.graph.RequestVar):
        return f"{Fore.BLUE}REQUEST{Fore.RESET} {format_options(api.OptionMap({change.request.var: change.request.value}))}"
    elif isinstance(change, solve.graph.SetPackageBuild):
        return f"{Fore.YELLOW}BUILD{Fore.RESET} {format_ident(change.spec.pkg)}"
    elif isinstance(change, solve.graph.SetPackage):
        return f"{Fore.GREEN}RESOLVE{Fore.RESET} {format_ident(change.spec.pkg)}"
    elif isinstance(change, solve.graph.SetOptions):
        return f"{Fore.CYAN}ASSIGN{Fore.RESET} {format_options(change.options)}"
    elif isinstance(change, solve.graph.StepBack):
        return f"{Fore.RED}BLOCKED{Fore.RESET} {change.cause}"
    else:
        return f"{Fore.MAGENTA}OTHER{Fore.RESET} {change}"


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
