// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

import io

from .. import encoding
from ._layer import Layer


def test_layer_encoding() -> None:

    expected = Layer(manifest=encoding.EMPTY_DIGEST)

    stream = io.BytesIO()
    expected.encode(stream)
    stream.seek(0)
    actual = Layer.decode(stream)
    assert isinstance(actual, Layer)
    assert actual.digest() == expected.digest()
