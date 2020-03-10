from typing import Iterable, Union
from colorama import Fore

from ._config import get_config
from . import storage, tracking, encoding


def format_digest(
    ref: Union[str, encoding.Digest], repo: storage.Repository = None
) -> str:
    """Return a nicely formatted string representation of the given reference."""

    if repo is None:
        config = get_config()
        repo = config.get_repository()

    try:
        aliases = repo.find_aliases(ref)
    except ValueError:
        aliases = []

    if isinstance(ref, encoding.Digest):
        ref = repo.get_shortened_digest(ref)
    return " -> ".join([ref] + aliases)


def format_diffs(diffs: Iterable[tracking.Diff]) -> str:
    """Return a human readable string rendering of the given diffs."""

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


def format_changes(diffs: Iterable[tracking.Diff]) -> str:
    """Return a string rendering of any given diffs which represent change."""

    diffs = filter(lambda x: x.mode is not tracking.DiffMode.unchanged, diffs)
    return format_diffs(diffs)
