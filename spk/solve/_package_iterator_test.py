import spfs

from .. import storage, api
from ._package_iterator import RepositoryPackageIterator, FilteredPackageIterator


def test_only_latest_release_is_given() -> None:

    repo = storage.MemRepository()
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.1"})
    repo.publish_spec(spec)
    spec.pkg.set_build("BGSHW3CN")
    repo.publish_package(
        spec, spfs.encoding.EMPTY_DIGEST,
    )
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.2"})
    repo.publish_spec(spec)
    spec.pkg.set_build("BGSHW3CN")
    repo.publish_package(
        spec, spfs.encoding.EMPTY_DIGEST,
    )
    spec.pkg.set_build("BVOFAV57")
    repo.publish_package(
        spec, spfs.encoding.EMPTY_DIGEST,
    )

    it = FilteredPackageIterator(
        RepositoryPackageIterator("my-pkg", [repo]),
        api.Request(api.parse_ident_range("my-pkg")),
        api.OptionMap(),
    )
    packages = list(it)
    assert len(packages) == 2, "Should return build of only release per package"
    for spec, _ in packages:
        assert (
            spec.pkg.version.post["r"] == 2
        ), "Should always present the latest release"


def test_old_release_allowed_if_requested() -> None:

    repo = storage.MemRepository()
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.1"})
    repo.publish_spec(spec)
    spec.pkg.set_build("BGSHW3CN")
    repo.publish_package(
        spec, spfs.encoding.EMPTY_DIGEST,
    )
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.2"})
    repo.publish_spec(spec)
    spec.pkg.set_build("BGSHW3CN")
    repo.publish_package(
        spec, spfs.encoding.EMPTY_DIGEST,
    )

    it = FilteredPackageIterator(
        RepositoryPackageIterator("my-pkg", [repo]),
        api.Request(api.parse_ident_range("my-pkg/=1+r.1")),
        api.OptionMap(),
    )
    packages = list(it)
    print(it.history)
    assert len(packages) == 1, "Should return one release per package"
    assert (
        packages[0][0].pkg.version.post["r"] == 1
    ), "Should present older releases when specifically asked"
