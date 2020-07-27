from ._option_map import OptionMap


def test_package_options() -> None:

    options = OptionMap()
    options["message"] = "hello, world"
    options["my-pkg.message"] = "hello, package"
    assert options.global_options() == OptionMap({"message": "hello, world"})
    assert options.package_options("my-pkg") == OptionMap({"message": "hello, package"})
