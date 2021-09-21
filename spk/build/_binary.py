from typing import List, Iterable, Optional, MutableMapping, Union
import os
import json
import subprocess

import structlog
import spkrs

from .. import api, storage, solve, exec
from ._env import data_path, deferred_signals

from spkrs.build import (
    build_options_path,
    build_script_path,
    build_spec_path,
    source_package_path,
)

_LOGGER = structlog.get_logger("spk.build")
