# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Iterable, Sequence, TextIO, Tuple, Union
from colorama import Fore, Style
import io
import sys

from . import api, pysolve, solve


def format_ident(pkg: api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts or pkg.build is not None:
        out += f"/{Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f"/{format_build(pkg.build)}"
    return out


def run_and_print_resolve(
    solver: Union[pysolve.legacy.Solver, pysolve.Solver, solve.Solver],
    verbosity: int = 1,
) -> solve.Solution:
    if isinstance(solver, pysolve.legacy.Solver):
        solution = solver.solve()
        print(format_decision_tree(solver.decision_tree, verbosity))
        return solution  # type: ignore
    elif isinstance(solver, pysolve.Solver):
        generator = solver.run()
        format_decisions(generator, out=sys.stdout)
        return generator.solution  # type: ignore
    else:
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

            if isinstance(change, (pysolve.graph.SetPackage, solve.graph.SetPackage)):
                if change.spec.pkg.build == api.EMBEDDED:
                    fill = "."
                else:
                    fill = ">"
            elif isinstance(change, (pysolve.graph.StepBack, solve.graph.StepBack)):
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
        pysolve.graph.SetPackage: 1,
        pysolve.graph.StepBack: 1,
        pysolve.graph.RequestPackage: 2,
        pysolve.graph.RequestVar: 2,
        pysolve.graph.SetOptions: 3,
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


def format_decision_tree(tree: pysolve.legacy.DecisionTree, verbosity: int = 1) -> str:

    out = ""
    for decision in tree.walk():
        out += ">" * decision.level()
        lines = format_decision(decision, verbosity).split("\n")
        out += " " + lines[0] + "\n"
        for line in lines[1:]:
            out += "." * decision.level()
            out += " " + line + "\n"
    return out[:-1]


def format_change(change: solve.graph.Change, _verbosity: int = 1) -> str:

    if isinstance(change, (pysolve.graph.RequestPackage, solve.graph.RequestPackage)):
        return f"{Fore.BLUE}REQUEST{Fore.RESET} {format_request(change.request.pkg.name, [change.request])}"
    elif isinstance(change, (pysolve.graph.RequestVar, solve.graph.RequestVar)):
        return f"{Fore.BLUE}REQUEST{Fore.RESET} {format_options(api.OptionMap({change.request.var: change.request.value}))}"
    elif isinstance(
        change, (pysolve.graph.SetPackageBuild, solve.graph.SetPackageBuild)
    ):
        return f"{Fore.YELLOW}BUILD{Fore.RESET} {format_ident(change.spec.pkg)}"
    elif isinstance(change, (pysolve.graph.SetPackage, solve.graph.SetPackage)):
        return f"{Fore.GREEN}RESOLVE{Fore.RESET} {format_ident(change.spec.pkg)}"
    elif isinstance(change, (pysolve.graph.SetOptions, solve.graph.SetOptions)):
        return f"{Fore.CYAN}ASSIGN{Fore.RESET} {format_options(change.options)}"
    elif isinstance(change, (pysolve.graph.StepBack, solve.graph.StepBack)):
        return f"{Fore.RED}BLOCKED{Fore.RESET} {change.cause}"
    else:
        return f"{Fore.MAGENTA}OTHER{Fore.RESET} {change}"


def format_note(note: solve.graph.Note) -> str:

    if isinstance(note, (pysolve.graph.SkipPackageNote, solve.graph.SkipPackageNote)):
        return f"{Fore.MAGENTA}TRY{Fore.RESET} {format_ident(note.pkg)} - {note.reason}"
    else:
        return f"{Fore.MAGENTA}NOTE{Fore.RESET} {note}"


def format_decision(decision: pysolve.legacy.Decision, verbosity: int = 1) -> str:

    end = "\n" if verbosity > 1 else " "
    out = ""

    error = decision.get_error()
    resolved = decision.get_resolved()
    requests = decision.get_requests()
    unresolved = decision.get_unresolved()
    if resolved:
        if verbosity > 1:
            for _, spec, _ in resolved.items():
                iterator = decision.get_iterator(spec.pkg.name)
                if iterator is not None:
                    versions = list(
                        f"{Fore.MAGENTA}TRY{Fore.RESET} {format_ident(v)} - {c}"
                        for v, c in iterator.get_history().items()
                    )
                    if versions:
                        out += end.join(reversed(versions)) + end
                out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {format_ident(spec.pkg)}" + end
                if verbosity > 2:
                    opt = spec.resolve_all_options(decision.get_options())
                    if opt:
                        out += format_options(opt) + end
        else:
            values = list(format_ident(spec.pkg) for _, spec, _ in resolved.items())
            out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {', '.join(values)}" + end
    if requests:
        values = list(format_request(n, pkgs) for n, pkgs in requests.items())
        out += f"{Fore.BLUE}REQUEST{Fore.RESET} {', '.join(values)}" + end
    if error is None and unresolved:
        if verbosity > 1:
            reasons = list(
                f"{Fore.YELLOW}UNRESOLVE{Fore.RESET} {v} - {c}"
                for v, c in unresolved.items()
            )
            if reasons:
                out += end.join(reversed(reasons)) + end
        else:
            out += f"{Fore.YELLOW}UNRESOLVE{Fore.RESET} {', '.join(unresolved)}" + end

    if error is not None:

        if not isinstance(error, pysolve.legacy.UnresolvedPackageError):
            out += f"{Fore.RED}BLOCKED{Fore.RESET} {error}"
        else:
            if verbosity > 1:
                versions = list(
                    f"{Fore.MAGENTA}TRY{Fore.RESET} {v} - {c}"
                    for v, c in (error.history or {}).items()
                )
                out += end.join(versions) + (end if versions else "")

            out += f"{Fore.RED}BLOCKED{Fore.RESET} {error.message}"

    return out.strip()


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
    if isinstance(err, solve.SolverFailedError):
        errors = err.graph.find_deepest_errors()
        if errors:
            msg += ", likely suspects:\n - " + ("\n - ".join(errors))
    if isinstance(err, solve.SolverError):
        if verbosity == 0:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '--verbose/-v' for more info"
        elif verbosity < 2:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '-vv' for even more info"
        elif verbosity < 3:
            msg += f"{Fore.YELLOW}{Style.DIM}\n * try '-vvv' for even more info"
    return f"{Fore.RED}{msg}{Style.RESET_ALL}"
