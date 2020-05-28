from typing import Dict, List
import io

import spfs
import pytest

from .. import api, storage
from ._errors import UnresolvedPackageError, ConflictingRequestsError, SolverError
from ._solver import Solver


def make_repo(
    specs: List[Dict], opts: api.OptionMap = api.OptionMap()
) -> storage.MemRepository:

    repo = storage.MemRepository()
    options = api.OptionMap()

    def add_pkg(spec_dict: Dict) -> None:
        spec = api.Spec.from_dict(spec_dict)
        repo.publish_spec(spec)
        repo.publish_package(
            spec.pkg.with_build(spec.resolve_all_options(options).digest()),
            spfs.encoding.EMPTY_DIGEST,
        )

    for spec in specs:
        add_pkg(spec)

    return repo


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

    repo = make_repo([{"pkg": "my_pkg/1.0.0"}])
    options = api.OptionMap()

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my_pkg")

    packages = solver.solve()
    assert len(packages) == 1, "expected one resolved package"
    assert packages["my_pkg"].version == "1.0.0"
    assert packages["my_pkg"].build is not None
    assert packages["my_pkg"].build.digest != api.SRC


def test_solver_single_package_simple_deps() -> None:

    options = api.OptionMap()
    repo = make_repo(
        [
            {"pkg": "pkg_a/0.9.0"},
            {"pkg": "pkg_a/1.0.0"},
            {"pkg": "pkg_a/1.2.0"},
            {"pkg": "pkg_a/1.2.1"},
            {"pkg": "pkg_a/2.0.0"},
            {"pkg": "pkg_b/1.0.0", "depends": [{"pkg": "pkg_a/2"}]},
            {"pkg": "pkg_b/1.1.0", "depends": [{"pkg": "pkg_a/1"}]},
        ]
    )

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("pkg_b/1.1")

    packages = solver.solve()
    assert len(packages) == 2, "expected two resolved packages"
    assert packages["pkg_a"].version == "1.2.1"
    assert packages["pkg_b"].version == "1.1.0"


def test_solver_dependency_incompatible() -> None:

    # test what happens when a dependency is added which is incompatible
    # with an existing request in the stack
    repo = make_repo(
        [
            {"pkg": "pkg_a/1.0.0"},
            {"pkg": "pkg_a/2.0.0"},
            {"pkg": "pkg_b/1.0.0", "depends": [{"pkg": "pkg_a/2"}]},
        ]
    )

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg_b/1")
    # this one is incompatible with pkg_b.depends but the solver doesn't know it yet
    solver.add_request("pkg_a/1")

    with pytest.raises(UnresolvedPackageError):
        solver.solve()

    for decision in solver.decision_tree.walk():
        print("." * decision.level(), decision)
        err = decision.get_error()
        if err is not None:
            assert isinstance(err, ConflictingRequestsError)
            break
    else:
        pytest.fail("expected to find problem with conflicting requests")


def test_solver_dependency_incompatible_stepback() -> None:

    # test what happens when a dependency is added which is incompatible
    # with an existing request in the stack - in this case we want the solver
    # to successfully step back into an older package version with
    # better dependencies
    repo = make_repo(
        [
            {"pkg": "pkg_a/1.0.0"},
            {"pkg": "pkg_a/2.0.0"},
            {"pkg": "pkg_b/1.1.0", "depends": [{"pkg": "pkg_a/2"}]},
            {"pkg": "pkg_b/1.0.0", "depends": [{"pkg": "pkg_a/1"}]},
        ]
    )

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg_b/1")
    # this one is incompatible with pkg_b/1.1.depends but not pkg_b/1.0
    solver.add_request("pkg_a/1")

    packages = solver.solve()
    assert packages["pkg_b"].version == "1.0.0"
    assert packages["pkg_a"].version == "1.0.0"


def test_solver_dependency_already_satisfied() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version satisfies the request

    repo = make_repo(
        [
            {
                "pkg": "pkg_top/1.0.0",
                # should resolve dep_1 as 1.0.0
                "depends": [{"pkg": "dep_1/1.0"}, {"pkg": "dep_2/1"}],
            },
            {"pkg": "dep_1/1.1.0"},
            {"pkg": "dep_1/1.0.0"},
            # when dep_2 gets resolved, it will re-request this but it has already resolved
            {"pkg": "dep_2/1.0.0", "depends": [{"pkg": "dep_1/1"}]},
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg_top")
    packages = solver.solve()
    assert list(packages.keys()) == ["pkg_top", "dep_1", "dep_2"]
    assert packages["dep_1"].version == "1.0.0"


def test_solver_dependency_reopen_solvable() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version does not satisfy the request
    #   - and a version exists for both (solvable)

    repo = make_repo(
        [
            {
                "pkg": "pkg_top/1.0.0",
                # should resolve dep_1 as 1.1.0 (favoring latest)
                "depends": [{"pkg": "dep_1/1"}, {"pkg": "dep_2/1"}],
            },
            {"pkg": "dep_1/1.1.0"},
            {"pkg": "dep_1/1.0.0"},
            # when dep_2 gets resolved, it will enforce an older version
            # of the existing resolve, which is still valid for all requests
            {"pkg": "dep_2/1.0.0", "depends": [{"pkg": "dep_1/1.0.0"}]},
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg_top")
    packages = solver.solve()
    assert list(packages.keys()) == ["pkg_top", "dep_2", "dep_1"]
    assert packages["dep_1"].version == "1.0.0"


def test_solver_dependency_reopen_unsolvable() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version does not satisfy the request
    #   - and a version does not exist for both (unsolvable)

    repo = make_repo(
        [
            {
                "pkg": "pkg_top/1.0.0",
                # must resolve dep_1 as 1.1.0 (favoring latest)
                "depends": [{"pkg": "dep_1/1.1"}, {"pkg": "dep_2/1"}],
            },
            {"pkg": "dep_1/1.1.0"},
            {"pkg": "dep_1/1.0.0"},
            # when dep_2 gets resolved, it will enforce an older version
            # of the existing resolve, which is in conflict with the original
            {"pkg": "dep_2/1.0.0", "depends": [{"pkg": "dep_1/1.0.0"}]},
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg_top")
    with pytest.raises(UnresolvedPackageError):
        packages = solver.solve()
        print(packages)
