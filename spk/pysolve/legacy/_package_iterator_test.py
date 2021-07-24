# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import spkrs

from ... import storage, api
from ._package_iterator import RepositoryPackageIterator, FilteredPackageIterator


def test_only_latest_release_is_given() -> None:

    repo = storage.MemRepository()
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.1"})
    repo.publish_spec(spec)
    spec.pkg = spec.pkg.with_build("BGSHW3CN")
    repo.publish_package(
        spec,
        spkrs.EMPTY_DIGEST,
    )
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.2"})
    repo.publish_spec(spec)
    spec.pkg = spec.pkg.with_build("BGSHW3CN")
    repo.publish_package(
        spec,
        spkrs.EMPTY_DIGEST,
    )
    spec.pkg = spec.pkg.with_build("BVOFAV57")
    repo.publish_package(
        spec,
        spkrs.EMPTY_DIGEST,
    )

    it = FilteredPackageIterator(
        RepositoryPackageIterator("my-pkg", [repo]),
        api.PkgRequest(api.parse_ident_range("my-pkg")),
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
    spec.pkg = spec.pkg.with_build("BGSHW3CN")
    repo.publish_package(
        spec,
        spkrs.EMPTY_DIGEST,
    )
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0+r.2"})
    repo.publish_spec(spec)
    spec.pkg = spec.pkg.with_build("BGSHW3CN")
    repo.publish_package(
        spec,
        spkrs.EMPTY_DIGEST,
    )

    it = FilteredPackageIterator(
        RepositoryPackageIterator("my-pkg", [repo]),
        api.PkgRequest(api.parse_ident_range("my-pkg/=1+r.1")),
        api.OptionMap(),
    )
    packages = list(it)
    assert len(packages) == 1, "Should return one release per package"
    assert (
        packages[0][0].pkg.version.post["r"] == 1
    ), "Should present older releases when specifically asked"
