# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Iterable, Union
import abc

from spkrs.storage import Repository

from .. import api


class VersionExistsError(FileExistsError):
    def __init__(self, pkg: Any) -> None:
        super(VersionExistsError, self).__init__(
            f"Package version already exists: {pkg}"
        )


class PackageNotFoundError(FileNotFoundError):
    def __init__(self, pkg: Any) -> None:
        super(PackageNotFoundError, self).__init__(f"Package not found: {pkg}")
