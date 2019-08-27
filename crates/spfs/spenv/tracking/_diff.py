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

    def __str__(self):
        return f"{self.mode.value} {self.path}"


def compute_diff(a: Manifest, b: Manifest) -> List[Diff]:

    changes: List[Diff] = []
    all_entries = EntryMap(itertools.chain(a.walk(), b.walk()))
    sort_entries(all_entries)

    for path in all_entries.keys():

        a_entry = a.get_path(path)
        b_entry = b.get_path(path)
        if a_entry is None:
            diff = Diff(mode=DiffMode.added, path=path)

        elif b_entry is None:
            diff = Diff(mode=DiffMode.removed, path=path)

        elif a_entry.digest == b_entry.digest:
            diff = Diff(mode=DiffMode.unchanged, path=path)

        else:
            diff = Diff(mode=DiffMode.changed, path=path)

        changes.append(diff)

    return changes
