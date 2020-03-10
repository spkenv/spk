from typing import List, NamedTuple
import enum
import itertools

from ._manifest import Manifest, EntryMap, sort_entries


class DiffMode(enum.Enum):

    unchanged = "="
    changed = "~"
    added = "+"
    removed = "-"


class Diff(NamedTuple):

    mode: DiffMode
    path: str

    def __str__(self) -> str:
        return f"{self.mode.value} {self.path}"


def compute_diff(a: Manifest, b: Manifest) -> List[Diff]:

    changes: List[Diff] = []
    all_entries = EntryMap(itertools.chain(a.walk(), b.walk()))
    sort_entries(all_entries)

    for path in all_entries.keys():

        diff = _diff_path(a, b, path)
        changes.append(diff)

    return changes


def _diff_path(a: Manifest, b: Manifest, path: str) -> Diff:

    try:
        a_entry = a.get_path(path)
    except FileNotFoundError:
        return Diff(mode=DiffMode.added, path=path)

    try:
        b_entry = b.get_path(path)
    except FileNotFoundError:
        return Diff(mode=DiffMode.removed, path=path)

    if a_entry.digest() == b_entry.digest():
        return Diff(mode=DiffMode.unchanged, path=path)

    return Diff(mode=DiffMode.changed, path=path)
