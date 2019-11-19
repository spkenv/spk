from typing import Sequence
from colorama import Fore

from ._config import get_config
from . import storage, tracking


def format_digest(ref: str, repo: storage.Repository = None) -> str:

    if repo is None:
        config = get_config()
        repo = config.get_repository()

    try:
        aliases = repo.find_aliases(ref)
    except ValueError:
        aliases = []
    return " -> ".join([ref] + aliases)


def format_diffs(diffs: Sequence[tracking.Diff]) -> str:

    outputs = []
    for diff in diffs:
        color = Fore.RESET
        if diff.mode == tracking.DiffMode.added:
            color = Fore.GREEN
        elif diff.mode == tracking.DiffMode.removed:
            color = Fore.RED
        elif diff.mode == tracking.DiffMode.changed:
            color = Fore.BLUE
        outputs.append(f"{color} {diff}{Fore.RESET}")

    return "\n".join(outputs)
