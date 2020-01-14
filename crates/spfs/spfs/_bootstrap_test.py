from typing import Any
import os
import subprocess

import pytest
import py.path

from . import runtime
from ._runtime import _which
from ._bootstrap import build_shell_initialized_command


@pytest.mark.parametrize(
    "shell,startup_cmd",
    (
        ("bash", "export TEST_VALUE='spfs-test-value'"),
        ("tcsh", "setenv TEST_VALUE 'spfs-test-value'"),
    ),
)
def test_shell_initialization_startup_scripts(
    shell: str, startup_cmd: str, tmpdir: py.path.local, monkeypatch: Any
) -> None:

    shell_path = _which(shell)
    if not shell_path:
        pytest.skip(f"{shell_path} not available on this system")

    storage = runtime.Storage(tmpdir.strpath)
    rt = storage.create_runtime("sh-test")

    monkeypatch.setenv("SPFS_RUNTIME", rt.root)
    monkeypatch.setenv("SHELL", shell_path)

    tmp_startup_dir = tmpdir.join("startup.d").ensure(dir=True)
    for startup_script in (rt.sh_startup_file, rt.csh_startup_file):
        print(
            subprocess.check_output(
                [
                    "sed",
                    "-i",
                    f"s|/spfs/etc/spfs/startup.d|{tmp_startup_dir.strpath}|",
                    startup_script,
                ]
            )
        )

    tmp_startup_dir.join("test.csh").write(startup_cmd, ensure=True)
    tmp_startup_dir.join("test.sh").write(startup_cmd, ensure=True)

    command = build_shell_initialized_command("printenv", "TEST_VALUE")
    out = subprocess.check_output(command)
    assert out.decode("utf-8").endswith("\nspfs-test-value\n")


@pytest.mark.parametrize("shell", ("bash", "tcsh"))
def test_shell_initialization_no_startup_scripts(
    shell: str, tmpdir: py.path.local, monkeypatch: Any
) -> None:

    shell_path = _which(shell)
    if not shell_path:
        pytest.skip(f"{shell_path} not available on this system")

    storage = runtime.Storage(tmpdir.strpath)
    rt = storage.create_runtime("sh-test")

    monkeypatch.setenv("SPFS_RUNTIME", rt.root)
    monkeypatch.setenv("SHELL", shell_path)

    tmp_startup_dir = tmpdir.join("startup.d").ensure(dir=True)
    for startup_script in (rt.sh_startup_file, rt.csh_startup_file):
        print(
            subprocess.check_output(
                [
                    "sed",
                    "-i",
                    f"s|/spfs/etc/spfs/startup.d|{tmp_startup_dir.strpath}|",
                    startup_script,
                ]
            )
        )

    command = build_shell_initialized_command("exit")
    out = subprocess.check_output(command)
    assert out.decode("utf-8") == ""
