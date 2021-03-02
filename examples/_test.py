import os
import sys
import subprocess
import glob
import tempfile
import itertools

import py.path
import pytest

import spk.cli
import spkrs

here = os.path.dirname(__file__)
testable_examples = glob.glob(f"{here}/**/*.spk.yaml", recursive=True)


@pytest.fixture(autouse=True, scope="session")
def tmpspfs() -> spk.storage.SpFSRepository:

    tmpdir = py.path.local(tempfile.mkdtemp())

    root = tmpdir.join("spfs_repo").strpath
    origin_root = tmpdir.join("spfs_origin").strpath
    config = spkrs.get_config()
    config.clear()
    config.add_section("storage")
    config.add_section("remote.origin")
    config.set("storage", "root", root)
    config.set("remote.origin", "address", "file:" + origin_root)
    spk.storage.SpFSRepository(origin_root)
    yield spk.storage.SpFSRepository(root)
    tmpdir.remove(rec=1)


@pytest.mark.parametrize(
    "stage,spec_file", itertools.product(("mks", "mkb", "test"), testable_examples)
)
def test_example(stage: str, spec_file: str) -> None:

    subprocess.check_call(
        [os.path.dirname(sys.executable) + "/spk", stage, "-vv", spec_file]
    )
