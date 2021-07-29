import itertools

import py.path

import spkrs
from ._spfs import SpFSRepository


def test_skip_invalid_builds(tmpdir: py.path.local) -> None:

    root = tmpdir.join("repo")
    root.join("renders").ensure_dir()
    root.join("objects").ensure_dir()
    root.join("payloads").ensure_dir()
    root.join("tags").ensure_dir()
    repo = SpFSRepository(spkrs.SpFSRepository("file:" + root.strpath))

    # generate some empty tag files to simply test that ones with
    # invalid build digests are ignored when iterating
    for kind, build in itertools.product(("spec", "pkg"), ("src", "invalid")):
        root.join("tags", "spk", kind, "mypkg", "1.0.0", build + ".tag").ensure()

    assert list(repo.list_package_builds("mypkg/1.0.0")) == [
        spkrs.api.parse_ident("mypkg/1.0.0/src")
    ]
