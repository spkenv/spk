from typing import Union

import spfs
from . import api, storage


def load_spec(pkg: Union[str, api.Ident]) -> api.Spec:
    """Load a package spec from the default repository."""

    if not isinstance(pkg, api.Ident):
        pkg = api.parse_ident(pkg)
    spfs_repo = spfs.get_config().get_repository()
    repo = storage.SpFSRepository(spfs_repo)
    return repo.read_spec(pkg)


def save_spec(spec: api.Spec) -> None:
    """Load a package spec from the default repository."""

    spfs_repo = spfs.get_config().get_repository()
    repo = storage.SpFSRepository(spfs_repo)
    repo.force_publish_spec(spec)
