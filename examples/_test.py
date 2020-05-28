import os
import subprocess

import py.path
import pytest

import spk.cli
import spfs

here = os.path.dirname(__file__)
examples = os.listdir(here)
examples.remove("_test.py")


@pytest.mark.parametrize("name", examples)
def test_make_source_package(name: str, tmpdir: py._path.local.LocalPath) -> None:

    spfs.get_config().set("storage", "root", tmpdir.strpath)

    subprocess.check_call(["spfs", "reset", "--edit", ""])
    os.chdir(os.path.join(here, name))
    try:
        args = spk.cli.parse_args(["make-source", "example.spk.yaml", "--no-runtime"])
        args.func(args)
        code = 0
    except SystemExit as e:
        code = e.code

    assert code == 0, "Make source failed for example"


@pytest.mark.parametrize("name", examples)
def test_make_binary_package(name: str, tmpdir: py._path.local.LocalPath) -> None:

    spfs.get_config().set("storage", "root", tmpdir.strpath)

    subprocess.check_call(["spfs", "reset", "--edit", ""])
    os.chdir(os.path.join(here, name))
    try:
        args = spk.cli.parse_args(["make-binary", "example.spk.yaml", "--no-runtime"])
        args.func(args)
        code = 0
    except SystemExit as e:
        code = e.code
    assert code == 0, "Make binary failed for example"
