// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

import io

from .. import graph, encoding
from ._platform import Platform


def test_platform_encoding() -> None:

    layers = (encoding.EMPTY_DIGEST, encoding.NULL_DIGEST)
    expected = Platform(stack=layers)

    stream = io.BytesIO()
    expected.encode(stream)
    print(stream.getvalue())
    stream.seek(0, io.SEEK_SET)
    actual = Platform.decode(stream)
    assert actual == expected
