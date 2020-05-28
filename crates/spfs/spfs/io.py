from typing import Iterable, Union
from colorama import Fore, Style

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
        ref = repo.objects.get_shortened_digest(ref)
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
            color = Fore.LIGHTBLUE_EX
        else:
            color = Style.DIM
        about = []
        for attr in ("mode", "object", "size"):
            a = getattr(diff.entries[0], attr)  # type: ignore
            b = getattr(diff.entries[1], attr)  # type: ignore
            if a != b:
                about.append(attr)
        outputs.append(
            f"{color} {Style.BRIGHT}{diff.mode.name:>8} {Style.NORMAL}/spfs{diff.path} {Style.DIM}[{','.join(about)}] {Style.RESET_ALL}"
        )

    return "\n".join(outputs)


def format_changes(diffs: Iterable[tracking.Diff]) -> str:
    """Return a string rendering of any given diffs which represent change."""

    diffs = filter(lambda x: x.mode is not tracking.DiffMode.unchanged, diffs)
    return format_diffs(diffs)


def format_size(size: float) -> str:
    """Return a human-readable file size in bytes."""
    for unit in ["B", "Ki", "Mi", "Gi", "Ti"]:
        if abs(size) < 1024.0:
            return f"{size:3.1f} {unit}"
        size /= 1024.0
    return f"{size:3.1f} Pi"
