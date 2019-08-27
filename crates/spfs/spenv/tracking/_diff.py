from typing import List, NamedTuple
import enum

from ._manifest import Manifest


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

    for path, b_entry in b.walk():

        a_entry = a.get_path(path)
        if a_entry is None:
            diff = Diff(mode=DiffMode.added, path=path)

        elif a_entry.digest == b_entry.digest:
            diff = Diff(mode=DiffMode.unchanged, path=path)

        else:
            diff = Diff(mode=DiffMode.changed, path=path)

        changes.append(diff)

    for path, a_entry in a.walk():
        other = b.get_path(path)
        if other is not None:
            continue

        diff = Diff(mode=DiffMode.removed, path=path)
        changes.append(diff)

    return changes
