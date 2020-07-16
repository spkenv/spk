import os
import subprocess
import glob
import tempfile

import py.path
import pytest

import spk.cli
import spfs

here = os.path.dirname(__file__)
testable_examples = set(os.listdir(here))
testable_examples ^= {"_test.py", "__pycache__"}


@pytest.fixture(autouse=True, scope="session")
def tmpspfs() -> spfs.storage.fs.FSRepository:

    tmpdir = py.path.local(tempfile.mkdtemp())

    root = tmpdir.join("spfs_repo").strpath
    origin_root = tmpdir.join("spfs_origin").strpath
    config = spfs.get_config()
    config.clear()
    config.add_section("storage")
    config.add_section("remote.origin")
    config.set("storage", "root", root)
    config.set("remote.origin", "address", "file:" + origin_root)
    spfs.storage.fs.FSRepository(origin_root, create=True)
    yield spfs.storage.fs.FSRepository(root, create=True)
    tmpdir.remove(rec=1)


@pytest.mark.parametrize("name", testable_examples)
def test_build_package(name: str) -> None:

    os.chdir(os.path.join(here, name))

    for filename in glob.glob("*.spk.yaml", recursive=False):

        subprocess.check_call(["spk", "build", "-v", filename])
