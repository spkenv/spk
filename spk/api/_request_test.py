from ._request import Request, PreReleasePolicy, InclusionPolicy


def test_prerelease_policy() -> None:

    a = Request.from_dict({"pkg": "something", "prereleasePolicy": "IncludeAll"})
    b = Request.from_dict({"pkg": "something", "prereleasePolicy": "ExcludeAll"})

    a.restrict(b)
    assert a.prerelease_policy is PreReleasePolicy.ExcludeAll


def test_inclusion_policy() -> None:

    a = Request.from_dict({"pkg": "something", "include": "IfAlreadyPresent"})
    b = Request.from_dict({"pkg": "something", "include": "Always"})

    a.restrict(b)
    assert a.inclusion_policy is InclusionPolicy.Always
