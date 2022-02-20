# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import List
import sys

import structlog
import colorama

import spkrs
from spkrs.exec import resolve_runtime_layers, setup_current_runtime, build_required_packages
from . import solve, storage, io, build, api

_LOGGER = structlog.get_logger("spk.exec")
