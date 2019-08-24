from typing import NamedTuple, List, Optional
import re
import json
import subprocess

from . import storage


def _resolve_lowerdirs(repo: storage.Repository, ref: str) -> List[str]:

    target = repo.read_ref(ref)
    if isinstance(target, storage.Runtime):
        parent_ref = target.get_parent_ref()
        if parent_ref is None:
            return []
        return _resolve_lowerdirs(repo, parent_ref)

    if isinstance(target, storage.Layer):
        return [target.diffdir]

    raise NotImplementedError(f"Unhandled ref type: {target}")
