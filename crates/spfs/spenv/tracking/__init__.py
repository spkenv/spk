from ._tag import Tag, TagSpec, decode_tag, parse_tag_spec
from ._manifest import (
    Manifest,
    Tree,
    Entry,
    EntryKind,
    compute_tree,
    compute_manifest,
    compute_entry,
)
from ._diff import Diff, DiffMode, compute_diff
