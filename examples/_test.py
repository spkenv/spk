# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Iterable
import os
import sys
import subprocess
import glob
import tempfile
import itertools

import py.path
import pytest

import spkrs
import spk.cli

here = os.path.dirname(__file__)
testable_examples = glob.glob(f"{here}/**/*.spk.yaml", recursive=True)


@pytest.fixture(autouse=True, scope="session")
def tmpspfs() -> Iterable[spkrs.storage.Repository]:

    tmpdir = py.path.local(tempfile.mkdtemp())
    root = tmpdir.join("spfs_repo").strpath
    os.environ["SPFS_STORAGE_ROOT"] = root
    # we rely on an outer runtime being created and it needs to still be found
    os.environ["SPFS_STORAGE_RUNTIMES"] = "/tmp/spfs-runtimes"
    if "SPFS_REMOTE_ORIGIN_ADDRESS" in os.environ:
        del os.environ["SPFS_REMOTE_ORIGIN_ADDRESS"]
    yield spkrs.storage.open_spfs_repository(root, create=True)
    tmpdir.remove(rec=1)


@pytest.mark.parametrize(
    "stage,spec_file", itertools.product(("mks", "mkb", "test"), testable_examples)
)
def test_example(stage: str, spec_file: str) -> None:

    try:
        spkrs.storage.remote_repository("origin")
    except FileNotFoundError:
        pytest.skip("examples depend on external packages")

    if "CI" in os.environ:
        pytest.skip("examples depend on external packages, and do not run in CI")

    subprocess.check_call(
        [
            os.path.dirname(sys.executable) + "/spk",
            stage,
            "-vvv",
            spec_file,
        ]
    )
