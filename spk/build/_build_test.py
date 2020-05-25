from typing import Any
import pytest
import py.path
import spfs

from ._build import validate_build_changeset, BuildError, execute_build


def test_validate_build_changeset_nothing() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset([])


def test_validate_build_changeset_modified() -> None:

    with pytest.raises(BuildError):

        validate_build_changeset(
            [
                spfs.tracking.Diff(
                    path="/spfs/file.txt", mode=spfs.tracking.DiffMode.changed
                )
            ]
        )


def test_execute_build(tmpdir: py.path.local, capfd: Any, monkeypatch: Any) -> None:

    fake_runtime = tmpdir.join("runtime")
    fake_runtime.join("startup.sh").write('"$@"; exit $?', ensure=True)
    monkeypatch.setenv("SPFS_RUNTIME", fake_runtime.strpath)

    build_script = tmpdir.join("build.sh")
    build_script.write("echo $PWD > /dev/stderr", ensure=True)
    execute_build(tmpdir.strpath, build_script.strpath)

    out, err = capfd.readouterr()
    assert err.strip() == tmpdir.strpath
