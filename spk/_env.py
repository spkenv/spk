# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import os

import spkrs
import structlog

from . import api, solve, storage
from spkrs import NoEnvironmentError, current_env

_LOGGER = structlog.get_logger("spk")
ACTIVE_PREFIX = os.getenv("SPK_ACTIVE_PREFIX", "/spfs")
ENV_FILENAME = ".spk-env.yaml"
