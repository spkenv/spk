from typing import List, Set

from .. import encoding
from ._database import DatabaseView, UnknownObjectError


def check_database_integrity(db: DatabaseView) -> List[Exception]:
    """Validate that all objects can be loaded and their children are accessible."""

    errors: List[Exception] = []
    visited: Set[encoding.Digest] = set()
    for obj in db.iter_objects():
        for digest in obj.child_objects():
            if digest in visited:
                continue
            try:
                child = db.read_object(digest)
            except Exception as e:
                errors.append(e)
    return errors
