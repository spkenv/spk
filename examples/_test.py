import os
import subprocess
import glob

import py.path
import pytest

import spk.cli
import spfs

here = os.path.dirname(__file__)
examples = os.listdir(here)
examples.remove("_test.py")
if "__pycache__" in examples:
    examples.remove("__pycache__")


@pytest.mark.parametrize("name", examples)
def test_make_source_package(name: str, tmpdir: py._path.local.LocalPath) -> None:

    spfs.get_config().set("storage", "root", tmpdir.strpath)

    os.chdir(os.path.join(here, name))

    for filename in glob.glob("*.spk.yaml", recursive=False):
        subprocess.check_call(["spfs", "reset", "--edit", ""])
        try:
            args = spk.cli.parse_args(["make-source", filename, "--no-runtime"])
            args.func(args)
            code = 0
        except SystemExit as e:
            code = e.code

        assert code == 0, "Make source failed for example"


@pytest.mark.parametrize("name", examples)
def test_make_binary_package(name: str, tmpdir: py._path.local.LocalPath) -> None:

    spfs.get_config().set("storage", "root", tmpdir.strpath)

    os.chdir(os.path.join(here, name))
    for filename in glob.glob("*.spk.yaml", recursive=False):
        subprocess.check_call(["spfs", "reset", "--edit", ""])
        try:
            args = spk.cli.parse_args(["make-binary", filename, "--no-runtime"])
            args.func(args)
            code = 0
        except SystemExit as e:
            code = e.code
        assert code == 0, "Make binary failed for example"
