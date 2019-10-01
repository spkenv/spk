from typing import Sequence
from colorama import Fore

import spenv

config = spenv.get_config()
repo = config.get_repository()


def format_digest(ref: str) -> str:

    try:
        aliases = repo.find_aliases(ref)
    except ValueError:
        aliases = []
    return " -> ".join([ref] + aliases)


def format_diffs(diffs: Sequence[spenv.tracking.Diff]) -> str:

    outputs = []
    for diff in diffs:
        color = Fore.RESET
        if diff.mode == spenv.tracking.DiffMode.added:
            color = Fore.GREEN
        elif diff.mode == spenv.tracking.DiffMode.removed:
            color = Fore.RED
        elif diff.mode == spenv.tracking.DiffMode.changed:
            color = Fore.BLUE
        outputs.append(f"{color} {diff}{Fore.RESET}")

    return "\n".join(outputs)
