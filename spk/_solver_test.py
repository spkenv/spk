from typing import Dict
import io

import spfs
import pytest

from . import api, storage
from ._nodes import BuildNode, BinaryPackageHandle
from ._solver import Solver, UnresolvedPackageError, Decision


def test_solver_package_with_no_spec() -> None:

    repo = storage.MemRepository()

    pkg = api.parse_ident("my_pkg/1.0.0")
    options = api.OptionMap()

    # publish package without publishing spec
    repo.publish_package(pkg.with_build(options.digest()), spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my_pkg")

    with pytest.raises(UnresolvedPackageError):
        solver.solve()


def test_solver_single_package_no_deps() -> None:

    repo = storage.MemRepository()
    options = api.OptionMap()
    spec = api.Spec.from_dict({"pkg": "my_pkg/1.0.0"})

    repo.publish_spec(spec)
    repo.publish_package(
        spec.pkg.with_build(options.digest()), spfs.encoding.EMPTY_DIGEST
    )

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my_pkg")

    packages = solver.solve()
    assert len(packages) == 1, "expected one resolved package"
    assert packages["my_pkg"].version == spec.pkg.version
    assert packages["my_pkg"].build is not None
    assert packages["my_pkg"].build.digest != api.SRC


def test_solver_single_package_simple_deps() -> None:

    repo = storage.MemRepository()
    options = api.OptionMap()

    def add_pkg(spec_dict: Dict) -> None:
        spec = api.Spec.from_dict(spec_dict)
        repo.publish_spec(spec)
        repo.publish_package(
            spec.pkg.with_build(spec.resolve_all_options(options).digest()),
            spfs.encoding.EMPTY_DIGEST,
        )

    add_pkg({"pkg": "pkg_a/0.9.0"})
    add_pkg({"pkg": "pkg_a/1.0.0"})
    add_pkg({"pkg": "pkg_a/1.2.0"})
    add_pkg({"pkg": "pkg_a/1.2.1"})
    add_pkg({"pkg": "pkg_a/2.0.0"})

    add_pkg({"pkg": "pkg_b/1.0.0", "depends": [{"pkg": "pkg_a/2"}]})
    add_pkg({"pkg": "pkg_b/1.1.0", "depends": [{"pkg": "pkg_a/1"}]})

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("pkg_b/1.1")

    packages = solver.solve()
    assert len(packages) == 2, "expected two resolved packages"
    assert packages["pkg_a"].version == "1.2.1"
    assert packages["pkg_b"].version == "1.1.0"


def test_decision_stack() -> None:

    base = Decision()
    top = Decision(base)

    base.add_request(api.parse_ident("my_pkg/1.0.0"))
    assert len(top.get_package_requests("my_pkg")) == 1

    top.add_request(api.parse_ident("my_pkg/1"))
    assert len(top.get_package_requests("my_pkg")) == 2


def test_request_merging() -> None:

    decision = Decision()
    decision.add_request(api.parse_ident("my_pkg/1"))
    decision.add_request(api.parse_ident("my_pkg/1.0.0"))
    decision.add_request(api.parse_ident("my_pkg/1.0"))

    assert decision.get_merged_request("my_pkg") == api.parse_ident("my_pkg/1.0.0")

    decision.add_request(api.parse_ident("my_pkg/1.0/src"))

    assert decision.get_merged_request("my_pkg") == api.parse_ident("my_pkg/1.0.0/src")
