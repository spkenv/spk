from .. import api, storage
from ._decision import Decision


def test_decision_stack() -> None:

    base = Decision()
    top = Decision(base)

    base.add_request(api.parse_ident("my-pkg/1.0.0"))
    assert len(top.get_package_requests("my-pkg")) == 1

    top.add_request(api.parse_ident("my-pkg/1"))
    assert len(top.get_package_requests("my-pkg")) == 2


def test_request_merging() -> None:

    decision = Decision()
    decision.add_request(api.parse_ident("my-pkg/1"))
    decision.add_request(api.parse_ident("my-pkg/1.0.0"))
    decision.add_request(api.parse_ident("my-pkg/1.0"))

    assert (
        str(decision.get_merged_request("my-pkg").pkg) == "my-pkg/1.0.0"
    )  # type: ignore

    decision.add_request(api.Request.from_dict({"pkg": "my-pkg/1.0/src"}))

    assert (
        str(decision.get_merged_request("my-pkg").pkg) == "my-pkg/1.0.0/src"
    )  # type: ignore


def test_decision_unresolved() -> None:

    decision = Decision()
    decision.add_request("a/1")
    decision.add_request("b/2")
    repo: storage.Repository = None  # type: ignore
    decision.set_resolved(api.Spec.from_dict({"pkg": "a/1"}), repo)
    assert "b" in decision.unresolved_requests()
