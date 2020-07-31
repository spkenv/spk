from typing import List
from colorama import Fore, Style

from . import api, storage, solve


def format_ident(pkg: api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts or pkg.build is not None:
        out += f"/{Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f"/{Style.DIM}{pkg.build}{Style.RESET_ALL}"
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
    if decision.get_error() is not None:

        err = decision.get_error()
        if err is None:
            return out
        if not isinstance(err, solve.UnresolvedPackageError):
            return f"{Fore.RED}BLOCKED{Fore.RESET} {err}"
        if verbosity > 1:
            versions = list(
                f"{Fore.MAGENTA}TRY{Fore.RESET} {v} - {c}"
                for v, c in (err.history or {}).items()
            )
            out += end.join(versions) + (end if versions else "")

        out += f"{Fore.RED}BLOCKED{Fore.RESET} {err.message}"
        return out

    resolved = decision.get_resolved()
    if resolved:

        if verbosity > 1:
            for _, spec, _ in resolved.items():
                iterator = decision.get_iterator(spec.pkg.name)
                if iterator is not None:
                    versions = list(
                        f"{Fore.MAGENTA}TRY{Fore.RESET} {format_ident(v)} - {c}"
                        for v, c in iterator.history.items()
                    )
                    out += end.join(reversed(versions)) + end
                out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {format_ident(spec.pkg)}" + end
        else:
            values = list(format_ident(spec.pkg) for _, spec, _ in resolved.items())
            out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {', '.join(values)}" + end
    if decision.get_requests():
        values = list(
            format_request(n, pkgs) for n, pkgs in decision.get_requests().items()
        )
        out += f"{Fore.BLUE}REQUEST{Fore.RESET} {', '.join(values)}" + end
    if decision.get_unresolved():
        out += (
            f"{Fore.RED}UNRESOLVE{Fore.RESET} {', '.join(decision.get_unresolved())}"
            + end
        )
    return out.strip()


def format_request(name: str, requests: List[api.Request]) -> str:

    out = f"{Style.BRIGHT}{name}{Style.RESET_ALL}/"
    versions = []
    for req in requests:
        ver = f"{Fore.LIGHTBLUE_EX}{str(req.pkg.version) or '*'}{Fore.RESET}"
        if req.pkg.build is not None:
            ver += f"/{Style.DIM}{req.pkg.build}{Style.RESET_ALL}"
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
