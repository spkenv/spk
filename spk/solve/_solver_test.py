from typing import Dict, List

import spfs
import pytest

from .. import api, storage, io
from ._errors import (
    UnresolvedPackageError,
    ConflictingRequestsError,
    SolverError,
    PackageNotFoundError,
)
from ._solver import Solver


def make_repo(
    specs: List[Dict], opts: api.OptionMap = api.OptionMap()
) -> storage.MemRepository:

    repo = storage.MemRepository()
    options = api.OptionMap()

    def add_pkg(spec_dict: Dict) -> None:
        spec = api.Spec.from_dict(spec_dict)
        repo.publish_spec(spec)
        spec.pkg.set_build(spec.resolve_all_options(options).digest())
        repo.publish_package(
            spec, spfs.encoding.EMPTY_DIGEST,
        )

    for spec in specs:
        add_pkg(spec)

    return repo


def test_solver_package_with_no_spec() -> None:

    repo = storage.MemRepository()

    options = api.OptionMap()
    spec = api.Spec.from_dict({"pkg": "my-pkg/1.0.0"})
    spec.pkg.set_build(options.digest())

    # publish package without publishing spec
    repo.publish_package(spec, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my-pkg")

    with pytest.raises(PackageNotFoundError):
        solver.solve()


def test_solver_single_package_no_deps() -> None:

    repo = make_repo([{"pkg": "my-pkg/1.0.0"}])
    options = api.OptionMap()

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my-pkg")

    try:
        packages = solver.solve()
    finally:
        print(io.format_decision_tree(solver.decision_tree, verbosity=100))
    assert len(packages) == 1, "expected one resolved package"
    assert packages.get("my-pkg").spec.pkg.version == "1.0.0"
    assert packages.get("my-pkg").spec.pkg.build is not None
    assert packages.get("my-pkg").spec.pkg.build.digest != api.SRC  # type: ignore


def test_solver_single_package_simple_deps() -> None:

    options = api.OptionMap()
    repo = make_repo(
        [
            {"pkg": "pkg-a/0.9.0"},
            {"pkg": "pkg-a/1.0.0"},
            {"pkg": "pkg-a/1.2.0"},
            {"pkg": "pkg-a/1.2.1"},
            {"pkg": "pkg-a/2.0.0"},
            {"pkg": "pkg-b/1.0.0", "install": {"requirements": [{"pkg": "pkg-a/2"}]}},
            {"pkg": "pkg-b/1.1.0", "install": {"requirements": [{"pkg": "pkg-a/1"}]}},
        ]
    )

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("pkg-b/1.1")

    try:
        packages = solver.solve()
    finally:
        print(io.format_decision_tree(solver.decision_tree, verbosity=100))
    assert len(packages) == 2, "expected two resolved packages"
    assert packages.get("pkg-a").spec.pkg.version == "1.2.1"
    assert packages.get("pkg-b").spec.pkg.version == "1.1.0"


def test_solver_dependency_incompatible() -> None:

    # test what happens when a dependency is added which is incompatible
    # with an existing request in the stack
    repo = make_repo(
        [
            {"pkg": "maya/2019.0.0"},
            {"pkg": "maya/2020.0.0"},
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "maya/2020"}]},
            },
        ]
    )

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("my-plugin/1")
    # this one is incompatible with requirements of my-plugin but the solver doesn't know it yet
    solver.add_request("maya/2019")

    with pytest.raises(UnresolvedPackageError):
        solver.solve()

    print(io.format_decision_tree(solver.decision_tree, verbosity=100))
    for decision in solver.decision_tree.walk():
        err = decision.get_error()
        if err is not None:
            assert isinstance(err, UnresolvedPackageError)
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
            {"pkg": "maya/2019"},
            {"pkg": "maya/2020"},
            {
                "pkg": "my-plugin/1.1.0",
                "install": {"requirements": [{"pkg": "maya/2020"}]},
            },
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "maya/2019"}]},
            },
        ]
    )

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("my-plugin/1")
    # this one is incompatible with requirements of my-plugin/1.1.0 but not my-plugin/1.0
    solver.add_request("maya/2019")

    try:
        packages = solver.solve()
    finally:
        print(io.format_decision_tree(solver.decision_tree, verbosity=100))
    assert packages.get("my-plugin").spec.pkg.version == "1.0.0"
    assert packages.get("maya").spec.pkg.version == "2019.0.0"


def test_solver_dependency_already_satisfied() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version satisfies the request

    repo = make_repo(
        [
            {
                "pkg": "pkg-top/1.0.0",
                # should resolve dep_1 as 1.0.0
                "install": {
                    "requirements": [{"pkg": "dep-1/~1.0.0"}, {"pkg": "dep-2/1"}]
                },
            },
            {"pkg": "dep-1/1.1.0"},
            {"pkg": "dep-1/1.0.0"},
            # when dep_2 gets resolved, it will re-request this but it has already resolved
            {"pkg": "dep-2/1.0.0", "install": {"requirements": [{"pkg": "dep-1/1"}]}},
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg-top")
    try:
        packages = solver.solve()
    finally:
        print(io.format_decision_tree(solver.decision_tree, verbosity=100))

    assert list(s.spec.pkg.name for s in packages.items()) == [
        "pkg-top",
        "dep-1",
        "dep-2",
    ]
    assert packages.get("dep-1").spec.pkg.version == "1.0.0"


def test_solver_dependency_reopen_solvable() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version does not satisfy the request
    #   - and a version exists for both (solvable)

    repo = make_repo(
        [
            {
                "pkg": "my-plugin/1.0.0",
                # should resolve maya as 2019.2 (favoring latest)
                "install": {
                    "requirements": [{"pkg": "maya/2019"}, {"pkg": "some-library/1"}]
                },
            },
            {"pkg": "maya/2019.2.0"},
            {"pkg": "maya/2019.0.0"},
            # when some-library gets resolved, it will enforce an older version
            # of the existing resolve, which is still valid for all requests
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2019.0.0"}]},
            },
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("my-plugin")
    try:
        packages = solver.solve()
    finally:
        print(io.format_decision_tree(solver.decision_tree, verbosity=100))
    assert list(s.spec.pkg.name for s in packages.items()) == [
        "my-plugin",
        "some-library",
        "maya",
    ]
    assert packages.get("maya").spec.pkg.version == "2019.0.0"


def test_solver_dependency_reopen_unsolvable() -> None:

    # test what happens when a dependency is added which represents
    # a package which has already been resolved
    # - and the resolved version does not satisfy the request
    #   - and a version does not exist for both (unsolvable)

    repo = make_repo(
        [
            {
                "pkg": "pkg-top/1.0.0",
                # must resolve dep_1 as 1.1.0 (favoring latest)
                "install": {"requirements": [{"pkg": "dep-1/1.1"}, {"pkg": "dep-2/1"}]},
            },
            {"pkg": "dep-1/1.1.0"},
            {"pkg": "dep-1/1.0.0"},
            # when dep_2 gets resolved, it will enforce an older version
            # of the existing resolve, which is in conflict with the original
            {
                "pkg": "dep-2/1.0.0",
                "install": {"requirements": [{"pkg": "dep-1/~1.0.0"}]},
            },
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("pkg-top")
    with pytest.raises(UnresolvedPackageError):
        packages = solver.solve()
        print(packages)


def test_solver_pre_release_config() -> None:

    repo = make_repo(
        [
            {"pkg": "my-pkg/0.9.0"},
            {"pkg": "my-pkg/1.0.0-pre.0"},
            {"pkg": "my-pkg/1.0.0-pre.1"},
            {"pkg": "my-pkg/1.0.0-pre.2"},
        ]
    )

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("my-pkg")

    solution = solver.solve()
    assert (
        solution.get("my-pkg").spec.pkg.version == "0.9.0"
    ), "should not resolve pre-release by default"

    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request(
        api.Request.from_dict({"pkg": "my-pkg", "prereleasePolicy": "IncludeAll"})
    )

    solution = solver.solve()
    assert solution.get("my-pkg").spec.pkg.version == "1.0.0-pre.2"


def test_solver_constraint_only() -> None:

    # test what happens when a dependency is marked as a constraint/optional
    # and no other request is added
    # - the constraint is noted
    # - the package does not get resolved into the final env

    repo = make_repo(
        [
            {
                "pkg": "vnp3/2.0.0",
                "install": {
                    "requirements": [
                        {"pkg": "python/3.7", "include": "IfAlreadyPresent"}
                    ]
                },
            },
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("vnp3")
    solution = solver.solve()

    with pytest.raises(KeyError):
        solution.get("python")


def test_solver_constraint_and_request() -> None:

    # test what happens when a dependency is marked as a constraint/optional
    # and also requested by another package
    # - the constraint is noted
    # - the constraint is merged with the request

    repo = make_repo(
        [
            {
                "pkg": "vnp3/2.0.0",
                "install": {
                    "requirements": [
                        {"pkg": "python/=3.7.3", "include": "IfAlreadyPresent"}
                    ]
                },
            },
            {
                "pkg": "my-tool/1.2.0",
                "install": {"requirements": [{"pkg": "vnp3"}, {"pkg": "python/3.7"}]},
            },
            {"pkg": "python/3.7.3"},
            {"pkg": "python/3.8.1"},
        ]
    )
    solver = Solver(api.OptionMap())
    solver.add_repository(repo)
    solver.add_request("my-tool")
    solution = solver.solve()
    print(io.format_decision_tree(solver.decision_tree, verbosity=100))

    assert solution.get("python").spec.pkg.version == "3.7.3"
