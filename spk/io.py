from re import L
from sys import version
from typing import List
from colorama import Fore, Style

from . import api, storage, solve


def format_ident(pkg: api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts or pkg.build is not None:
        out += f"/{Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f"/{format_build(pkg.build)}"
    return out


def format_decision_tree(tree: solve.DecisionTree, verbosity: int = 1) -> str:

    out = ""
    for decision in tree.walk():
        out += ">" * decision.level()
        lines = format_decision(decision, verbosity).split("\n")
        out += " " + lines[0] + "\n"
        for line in lines[1:]:
            out += "." * decision.level()
            out += " " + line + "\n"
    return out[:-1]


def format_decision(decision: solve.Decision, verbosity: int = 1) -> str:

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
                if isinstance(iterator, solve.RepositoryPackageIterator):
                    versions = list(
                        f"{Fore.MAGENTA}TRY{Fore.RESET} {format_ident(v)} - {c}"
                        for v, c in iterator.history.items()
                    )
                    if versions:
                        out += end.join(reversed(versions)) + end
                out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {format_ident(spec.pkg)}" + end
        else:
            values = list(format_ident(spec.pkg) for _, spec, _ in resolved.items())
            out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {', '.join(values)}" + end
    if requests:
        values = list(format_request(n, pkgs) for n, pkgs in requests.items())
        out += f"{Fore.BLUE}REQUEST{Fore.RESET} {', '.join(values)}" + end
    if unresolved:
        out += f"{Fore.YELLOW}UNRESOLVE{Fore.RESET} {', '.join(unresolved)}" + end

    if error is not None:

        if not isinstance(error, solve.UnresolvedPackageError):
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


def format_request(name: str, requests: List[api.Request]) -> str:

    out = f"{Style.BRIGHT}{name}{Style.RESET_ALL}/"
    versions = []
    for req in requests:
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


def format_build(build: api.Build) -> str:

    if build.is_emdeded():
        return f"{Fore.LIGHTMAGENTA_EX}{build}{Style.RESET_ALL}"
    elif build.is_source():
        return f"{Fore.LIGHTYELLOW_EX}{build}{Style.RESET_ALL}"
    else:
        return f"{Style.DIM}{build}{Style.RESET_ALL}"
