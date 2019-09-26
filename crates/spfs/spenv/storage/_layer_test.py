from .. import tracking
from ._layer import Layer


def test_read_layer() -> None:

    expected = Layer(manifest=tracking.Manifest(), environ=tuple())
    data = expected.dump_dict()
    actual = Layer.load_dict(data)
    assert isinstance(actual, Layer)
    assert actual == expected
