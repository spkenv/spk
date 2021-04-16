# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from ._request import PkgRequest, PreReleasePolicy, InclusionPolicy


def test_prerelease_policy() -> None:

    a = PkgRequest.from_dict({"pkg": "something", "prereleasePolicy": "IncludeAll"})
    b = PkgRequest.from_dict({"pkg": "something", "prereleasePolicy": "ExcludeAll"})

    a.restrict(b)
    assert a.prerelease_policy is PreReleasePolicy.ExcludeAll


def test_inclusion_policy() -> None:

    a = PkgRequest.from_dict({"pkg": "something", "include": "IfAlreadyPresent"})
    b = PkgRequest.from_dict({"pkg": "something", "include": "Always"})

    a.restrict(b)
    assert a.inclusion_policy is InclusionPolicy.Always
