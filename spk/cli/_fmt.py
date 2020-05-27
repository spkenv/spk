from colorama import Fore, Style

import spk


def format_ident(pkg: spk.api.Ident) -> str:

    out = f"{Style.BRIGHT}{pkg.name}{Style.RESET_ALL}"
    if pkg.version.parts:
        out += f" / {Fore.LIGHTBLUE_EX}{pkg.version}{Fore.RESET}"
    if pkg.build is not None:
        out += f" / {Style.DIM}{pkg.build}{Style.RESET_ALL}"
    return out
