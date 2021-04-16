# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

"""Main entry points and utilities for command line interface and interaction."""

from ._run import main, run
from ._args import parse_args, configure_logging, configure_sentry
