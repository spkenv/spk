from typing import List
from colorama import Fore, Style

import spk


def format_ident(pkg: spk.api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts or pkg.build is not None:
        out += f"/{Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f"/{Style.DIM}{pkg.build}{Style.RESET_ALL}"
    return out


def format_decision_tree(tree: spk.DecisionTree) -> str:

    out = ""
    for decision in tree.walk():
        out += ">" * decision.level()
        out += " " + format_decision(decision) + "\n"
    return out[:-1]


def format_decision(decision: spk.Decision) -> str:

    if decision.get_error() is not None:
        return f"{Fore.RED}BLOCKED{Fore.RESET} {decision.get_error()}"
    out = ""
    if decision.get_resolved():
        values = list(
            format_ident(spec.pkg) for _, spec, _ in decision.get_resolved().items()
        )
        out += f"{Fore.GREEN}RESOLVE{Fore.RESET} {', '.join(values)} "
    if decision.get_requests():
        values = list(
            format_request(n, pkgs) for n, pkgs in decision.get_requests().items()
        )
        out += f"{Fore.BLUE}REQUEST{Fore.RESET} {', '.join(values)} "
    if decision.get_unresolved():
        out += (
            f"{Fore.RED}UNRESOLVE{Fore.RESET} {', '.join(decision.get_unresolved())} "
        )
    return out


def format_request(name: str, requests: List[spk.api.Request]) -> str:

    out = f"{Style.BRIGHT}{name}{Style.RESET_ALL}/"
    versions = []
    for req in requests:
        ver = f"{Fore.LIGHTBLUE_EX}{str(req.pkg.version) or '*'}{Fore.RESET}"
        if req.pkg.build is not None:
            ver += f"/{Style.DIM}{req.pkg.build}{Style.RESET_ALL}"
        versions.append(ver)
    out += ",".join(versions)
    return out
