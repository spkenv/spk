"""Create and manage file system layers."""
from typing import List
import os
import sys
import errno
import subprocess

from . import storage
from ._runtime import mount, unmount


def commit(ref) -> storage.Layer:

    repo = storage.configured_repository()
    return repo.commit(ref)
