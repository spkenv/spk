from ._platform import Platform


def test_platform_dump_load_dict() -> None:

    layers = ("a", "b", "c")
    expected = Platform(layers=layers)
    data = expected.dump_dict()
    actual = Platform.load_dict(data)
    assert actual == expected
