// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

from typing import Any, BinaryIO, Tuple, Optional
import time
import multiprocessing
import itertools
from unittest import mock

import pytest
import py.path

from ... import encoding, graph
from .. import Blob
from ._database import FSDatabase, _OBJECT_KINDS
import random


class LargeObj(graph.Object):
    def digest(self) -> encoding.Digest:
        # all objs share one digest
        return encoding.NULL_DIGEST

    def encode(self, writer: BinaryIO) -> None:

        # simlulate a long write process
        time.sleep(2)

    @classmethod
    def decode(self, reader: BinaryIO) -> "LargeObj":
        return LargeObj()


def try_sync(db: FSDatabase) -> Optional[Exception]:
    try:
        obj = LargeObj()
        if not db.has_object(obj.digest()):
            db.write_object(LargeObj())
    except Exception as e:
        return e
    return None


def test_database_race_condition(tmpdir: py.path.local) -> None:

    db = FSDatabase(tmpdir.strpath)

    with mock.patch.dict(_OBJECT_KINDS, {99: LargeObj}):

        with multiprocessing.Pool() as pool:
            results = pool.map(try_sync, itertools.repeat(db, 50))
            for err in results:
                if err is not None:
                    pytest.fail(str(err))
