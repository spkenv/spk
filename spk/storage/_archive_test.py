# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import List
import py.path
import getpass
import tarfile

from .. import build, api, storage
from ._archive import export_package, import_package


def test_archive_io(tmpdir: py.path.local) -> None:

    spec = api.Spec.from_dict(
        {
            "pkg": "spk-archive-test/0.0.1",
            "build": {"script": "touch /spfs/file.txt"},
        }
    )
    repo = storage.local_repository()
    repo.publish_spec(spec)
    builder = build.BinaryPackageBuilder.from_spec(spec).with_source(".")
    spec = builder.build()
    filename = tmpdir.join("achive.spk").ensure().strpath
    export_package(spec.pkg, filename)
    actual: List[str] = []
    with tarfile.open(filename) as tar:
        for file in tar:
            actual.append(file.name)
    actual.sort()
    assert actual == [
        ".",
        "VERSION",
        "objects",
        "objects/2A",
        "objects/2A/S5IXG2ITIIPTN7HMSVOZISP4QUAJBP2F6UGC7J5SUNKHPYFLPA====",
        "objects/4O",
        "objects/4O/YMIQUY7QOBJGX36TEJS35ZEQT24QPEMSNZGTFESWMRW6CSXBKQ====",
        "objects/DA",
        "objects/DA/N6HR2JME3TM7VMMGIDDSPXHEC4UJ23IQGJXPKTCB7TKQXYDCYA====",
        "objects/DF",
        "objects/DF/JBAGJDNNOMOYSD6WAATGNMBVEUUHNNMLGUXMADSGKEUKCZQ7ZA====",
        "objects/IQ",
        "objects/IQ/JW7I2VWNTYUEKGVULPP2DET2KPWT6CD7TX5AYQYBQPMHFK76FA====",
        "objects/KU",
        "objects/KU/7BPQ3TRMX65RJSJJ2H36JZZW3SHQA6QVNBFA4IYCQHQMCQYG5Q====",
        "objects/Y7",
        "objects/Y7/5TF2HNNMMJRXZ3HNUVXENZZZOWDQSIBJANAWF6J6WOFRVWEVPA====",
        "objects/work",
        "payloads",
        "payloads/2A",
        "payloads/2A/S5IXG2ITIIPTN7HMSVOZISP4QUAJBP2F6UGC7J5SUNKHPYFLPA====",
        "payloads/4O",
        "payloads/4O/YMIQUY7QOBJGX36TEJS35ZEQT24QPEMSNZGTFESWMRW6CSXBKQ====",
        "payloads/IQ",
        "payloads/IQ/JW7I2VWNTYUEKGVULPP2DET2KPWT6CD7TX5AYQYBQPMHFK76FA====",
        "payloads/KU",
        "payloads/KU/7BPQ3TRMX65RJSJJ2H36JZZW3SHQA6QVNBFA4IYCQHQMCQYG5Q====",
        "payloads/Y7",
        "payloads/Y7/5TF2HNNMMJRXZ3HNUVXENZZZOWDQSIBJANAWF6J6WOFRVWEVPA====",
        "payloads/work",
        "renders",
        f"renders/{getpass.getuser()}",
        "tags",
        "tags/spk",
        "tags/spk/pkg",
        "tags/spk/pkg/spk-archive-test",
        "tags/spk/pkg/spk-archive-test/0.0.1",
        "tags/spk/pkg/spk-archive-test/0.0.1/3I42H3S6.tag",
        "tags/spk/spec",
        "tags/spk/spec/spk-archive-test",
        "tags/spk/spec/spk-archive-test/0.0.1",
        "tags/spk/spec/spk-archive-test/0.0.1.tag",
        "tags/spk/spec/spk-archive-test/0.0.1/3I42H3S6.tag",
    ]
    import_package(filename)
