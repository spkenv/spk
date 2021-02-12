from conftest import with_install
import subprocess

import py.path


def test_runtime_file_removal(tmpdir: py.path.local, with_install: None) -> None:

    script = tmpdir.join("script.sh")
    filename = "/spfs/message.txt"
    base_tag = "test/file_removal_base"
    top_tag = "test/file_removal_top"
    script.write(
        "\n".join(
            [
                f"spfs run - bash -c 'echo hello > {filename} && spfs commit layer -t {base_tag}'",
                f"spfs run -e {base_tag} -- bash -c 'rm {filename} && spfs commit platform -t {top_tag}'",
                f"spfs run {top_tag} -- test ! -f {filename}",
            ]
        ),
        ensure=True,
    )
    subprocess.check_call(["bash", "-ex", script.strpath])


def test_runtime_dir_removal(tmpdir: py.path.local, with_install: None) -> None:

    script = tmpdir.join("script.sh")
    dirpath = "/spfs/dir1/dir2/dir3"
    to_remove = "/spfs/dir1/dir2"
    to_remain = "/spfs/dir1"
    base_tag = "test/dir_removal_base"
    top_tag = "test/dir_removal_top"
    script.write(
        "\n".join(
            [
                f"spfs run - bash -c 'mkdir -p {dirpath} && spfs commit layer -t {base_tag}'",
                f"spfs run -e {base_tag} -- bash -c 'rm -r {to_remove} && spfs commit platform -t {top_tag}'",
                f"spfs run {top_tag} -- test ! -d {to_remove}",
                f"spfs run {top_tag} -- test -d {to_remain}",
            ]
        ),
        ensure=True,
    )
    subprocess.check_call(["bash", "-ex", script.strpath])


def test_runtime_recursion(with_install: None) -> None:

    out = subprocess.check_output(
        [
            "spfs",
            "run",
            "",
            "--",
            "sh",
            "-c",
            "spfs edit --off; spfs run - -- echo hello",
        ]
    )
    assert out.decode() == "hello\n"
