from typing import List, NamedTuple, Optional, Tuple
import enum
import posixpath
import itertools

from ._entry import Entry
from ._manifest import Manifest, NodeMap, sort_entries

#[cfg(test)]
#[path = "./diff_test.rs"]
mod diff_test;

class DiffMode(enum.Enum):

    unchanged = "="
    changed = "~"
    added = "+"
    removed = "-"


class Diff(NamedTuple):

    mode: DiffMode
    path: str
    entries: Optional[Tuple[Entry, Entry]] = None

    def __str__(self) -> str:
        return f"{self.mode.value} {self.path}{self.details()}"

    def details(self) -> str:

        details = ""
        if self.entries is None:
            return details
        a, b = self.entries
        if a.mode != b.mode:
            details += f" {{{a.mode:06o} => {b.mode:06o}}}"
        if a.kind != b.kind:
            details += f" {{{a.kind.value()} => {b.kind.value()}}}"
        if a.object != b.object:
            details += " {!object!}"
        return details


def compute_diff(a: Manifest, b: Manifest) -> List[Diff]:

    changes: List[Diff] = []
    all_entries = NodeMap(itertools.chain(a.walk(), b.walk()))
    sort_entries(all_entries)

    for path in all_entries.keys():

        diff = _diff_path(a, b, path)
        changes.append(diff)

    return changes


def _diff_path(a: Manifest, b: Manifest, path: str) -> Diff:

    try:
        a_entry = a.get_path(path)
    except (FileNotFoundError, NotADirectoryError):
        return Diff(mode=DiffMode.added, path=path)

    try:
        b_entry = b.get_path(path)
    except (FileNotFoundError, NotADirectoryError):
        return Diff(mode=DiffMode.removed, path=path)

    name = posixpath.basename(path)
    if a_entry == b_entry:
        return Diff(mode=DiffMode.unchanged, path=path)

    return Diff(mode=DiffMode.changed, path=path, entries=(a_entry, b_entry))
