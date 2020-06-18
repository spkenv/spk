from ._request import Request, PreReleasePolicy


def test_prerelease_policy():

    a = Request.from_dict({"pkg": "something", "prereleasePolicy": "IncludeAll"})
    b = Request.from_dict({"pkg": "something", "prereleasePolicy": "ExcludeAll"})

    a.restrict(b)
    assert a.prerelease_policy is PreReleasePolicy.ExcludeAll
