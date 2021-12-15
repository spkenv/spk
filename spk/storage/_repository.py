# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Iterable, Union
import abc

from spkrs.storage import Repository

from .. import api


VersionExistsError = FileExistsError
PackageNotFoundError = FileNotFoundError
