from unittest import mock

import py.path
import pytest

from . import tracking, runtime
from ._commit import commit_layer, commit_platform, NothingToCommitError


def test_commit_empty(tmpdir: py.path.local) -> None:

    rt = runtime.Runtime(tmpdir.strpath)
    with pytest.raises(NothingToCommitError):
        commit_layer(rt)

    with pytest.raises(NothingToCommitError):
        commit_platform(rt)
