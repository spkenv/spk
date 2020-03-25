import py.path

from ._payloads import makedirs_with_perms


def test_makedirs_dont_change_existing(tmpdir: py.path.local) -> None:

    chkdir = tmpdir.join("my_dir")
    chkdir.ensure(dir=1)
    chkdir.chmod(755)
    original = chkdir.stat().mode
    makedirs_with_perms(chkdir + "/new", perms=0o777)
    assert chkdir.stat().mode == original, "existing dir should not change perms"
