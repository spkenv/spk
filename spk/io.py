# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Iterable, Sequence, TextIO, Tuple, Union
from colorama import Fore, Style
import io
import sys

from . import api, solve


def format_ident(pkg: api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts or pkg.build is not None:
        out += f"/{Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f"/{format_build(pkg.build)}"
    return out


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


def format_note(note: solve.graph.Note) -> str:

    if isinstance(note, solve.graph.SkipPackageNote):
        return f"{Fore.MAGENTA}TRY{Fore.RESET} {format_ident(note.pkg)} - {note.reason}"
    else:
        return f"{Fore.MAGENTA}NOTE{Fore.RESET} {note}"


def format_request(name: str, requests: Sequence[api.Request]) -> str:

    out = f"{Style.BRIGHT}{name}{Style.RESET_ALL}/"
    versions = []
    for req in requests:
        assert isinstance(
            req, api.PkgRequest
        ), f"TODO: Unhandled request in formatter {type(req)}"
        ver = f"{Fore.LIGHTBLUE_EX}{str(req.pkg.version) or '*'}{Fore.RESET}"
        if req.pkg.build is not None:
            ver += f"/{format_build(req.pkg.build)}"
        versions.append(ver)
    out += ",".join(versions)
    return out


def format_options(options: api.OptionMap) -> str:

    formatted = []
    for name, value in options.items():
        formatted.append(
            f"{name}{Style.DIM}={Style.NORMAL}{Fore.CYAN}{value}{Fore.RESET}"
        )

    return f"{{{', '.join(formatted)}}}"


def format_build(build: str) -> str:

    if build == api.EMBEDDED:
        return f"{Fore.LIGHTMAGENTA_EX}{build}{Style.RESET_ALL}"
    elif build == api.SRC:
        return f"{Fore.LIGHTYELLOW_EX}{build}{Style.RESET_ALL}"
    else:
        return f"{Style.DIM}{build}{Style.RESET_ALL}"


def format_solution(solution: solve.Solution, verbosity: int = 0) -> str:

    out = "Installed Packages:\n"
    for _, spec, _ in solution.items():
        if verbosity:
            options = spec.resolve_all_options(api.OptionMap({}))
            out += f"  {format_ident(spec.pkg)} {format_options(options)}\n"
        else:
            out += f"  {format_ident(spec.pkg)}\n"
    return out


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
