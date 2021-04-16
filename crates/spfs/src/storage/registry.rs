// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

from typing import Dict, Callable
import urllib.parse

from ._repository import Repository

RepositoryFactory = Callable[[str], Repository]

_FACTORIES: Dict[str, RepositoryFactory] = {}


def register_scheme(scheme: str, factory: RepositoryFactory) -> None:
    """Register a repository factory for a url scheme."""

    _FACTORIES[scheme] = factory


def open_repository(address: str) -> Repository:

    url = urllib.parse.urlparse(address)

    if url.scheme not in _FACTORIES:
        raise ValueError("unsupported repository scheme: " + address)

    factory = _FACTORIES[url.scheme]
    return factory(address)
