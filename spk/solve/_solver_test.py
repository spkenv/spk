# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Dict, List, Union

import spkrs
import pytest

from .. import api, storage, io
from . import legacy
from ._errors import (
    SolverError,
    PackageNotFoundError,
)
from ._solver import Solver


@pytest.fixture(params=["legacy", "graph"])
def solver(request: Any) -> Union[legacy.Solver, Solver]:

    if request.param == "legacy":
        return legacy.Solver({})
    else:
        return Solver()


def make_repo(
    specs: List[Union[Dict, api.Spec]], opts: api.OptionMap = api.OptionMap()
) -> storage.MemRepository:

    repo = storage.MemRepository()

    def add_pkg(s: Union[Dict, api.Spec]) -> None:
        if isinstance(s, dict):
            spec = api.Spec.from_dict(s)
            s = spec.copy()
            spec.pkg = spec.pkg.with_build(None)
            repo.force_publish_spec(spec)
            s = make_build(s.to_dict(), [], opts)
        repo.publish_package(s, spkrs.EMPTY_DIGEST)

    for s in specs:
        add_pkg(s)

    return repo


def make_build(
    spec_dict: Dict, deps: List[api.Spec] = [], opts: api.OptionMap = api.OptionMap()
) -> api.Spec:

    spec = api.Spec.from_dict(spec_dict)
    if spec.pkg.build == api.SRC:
        return spec
    build_opts = opts.copy()
    build_opts.update(spec.resolve_all_options(build_opts))
    spec.update_spec_for_build(build_opts, deps)
    return spec


def test_solver_no_requests(solver: Union[Solver, legacy.Solver]) -> None:

    solver.solve()


def test_solver_package_with_no_spec(solver: Union[Solver, legacy.Solver]) -> None:

    repo = storage.MemRepository()

    options = api.OptionMap()
    spec = api.Spec.from_dict({"pkg": f"my-pkg/1.0.0/{options.digest}"})

    # publish package without publishing spec
    repo.publish_package(spec, spkrs.EMPTY_DIGEST)

    solver.update_options(options)
    solver.add_repository(repo)
    solver.add_request("my-pkg")

    with pytest.raises((PackageNotFoundError, SolverError)):  # type: ignore
        solver.solve()
        print(io.format_resolve(solver, 999))


def test_solver_single_package_no_deps(solver: Union[Solver, legacy.Solver]) -> None:

    options = api.OptionMap()
    repo = make_repo([{"pkg": "my-pkg/1.0.0"}], options)

    solver.update_options(options)
    solver.add_repository(repo)
    solver.add_request("my-pkg")

    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert len(packages) == 1, "expected one resolved package"
    assert packages.get("my-pkg").spec.pkg.version == "1.0.0"
    assert packages.get("my-pkg").spec.pkg.build is not None
    assert packages.get("my-pkg").spec.pkg.build != api.SRC  # type: ignore


def test_solver_single_package_simple_deps(
    solver: Union[Solver, legacy.Solver]
) -> None:

    options = api.OptionMap()
    repo = make_repo(
        [
            {"pkg": "pkg-a/0.9.0"},
            {"pkg": "pkg-a/1.0.0"},
            {"pkg": "pkg-a/1.2.0"},
            {"pkg": "pkg-a/1.2.1"},
            {"pkg": "pkg-a/2.0.0"},
            {"pkg": "pkg-b/1.0.0", "install": {"requirements": [{"pkg": "pkg-a/2.0"}]}},
            {"pkg": "pkg-b/1.1.0", "install": {"requirements": [{"pkg": "pkg-a/1.2"}]}},
        ]
    )

    solver.update_options(options)
    solver.add_repository(repo)
    solver.add_request("pkg-b/1.1")

    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert len(packages) == 2, "expected two resolved packages"
    assert packages.get("pkg-a").spec.pkg.version == "1.2.1"
    assert packages.get("pkg-b").spec.pkg.version == "1.1.0"


def test_solver_dependency_abi_compat(solver: Union[Solver, legacy.Solver]) -> None:

    options = api.OptionMap()
    repo = make_repo(
        [
            {
                "pkg": "pkg-b/1.1.0",
                "install": {"requirements": [{"pkg": "pkg-a/1.1.0"}]},
            },
            {"pkg": "pkg-a/2.1.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.2.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.1.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.1.0", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.0.0", "compat": "x.a.b"},
            {"pkg": "pkg-a/0.9.0", "compat": "x.a.b"},
        ]
    )

    solver.update_options(options)
    solver.add_repository(repo)
    solver.add_request("pkg-b/1.1")

    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert len(packages) == 2, "expected two resolved packages"
    assert packages.get("pkg-a").spec.pkg.version == "1.1.1"
    assert packages.get("pkg-b").spec.pkg.version == "1.1.0"


def test_solver_dependency_incompatible(solver: Union[Solver, legacy.Solver]) -> None:

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

    solver.add_repository(repo)
    solver.add_request("my-plugin/1")
    # this one is incompatible with requirements of my-plugin but the solver doesn't know it yet
    solver.add_request("maya/2019")

    with pytest.raises(SolverError):
        solver.solve()

    print(io.format_resolve(solver, verbosity=100))


def test_solver_dependency_incompatible_stepback(
    solver: Union[Solver, legacy.Solver]
) -> None:

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

    solver.add_repository(repo)
    solver.add_request("my-plugin/1")
    # this one is incompatible with requirements of my-plugin/1.1.0 but not my-plugin/1.0
    solver.add_request("maya/2019")

    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert packages.get("my-plugin").spec.pkg.version == "1.0.0"
    assert packages.get("maya").spec.pkg.version == "2019.0.0"


def test_solver_dependency_already_satisfied(
    solver: Union[Solver, legacy.Solver]
) -> None:

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
    solver.add_repository(repo)
    solver.add_request("pkg-top")
    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert list(s.spec.pkg.name for s in packages.items()) == [
        "pkg-top",
        "dep-1",
        "dep-2",
    ]
    assert packages.get("dep-1").spec.pkg.version == "1.0.0"


def test_solver_dependency_reopen_solvable(
    solver: Union[Solver, legacy.Solver]
) -> None:

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
    solver.add_repository(repo)
    solver.add_request("my-plugin")
    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert set(s.spec.pkg.name for s in packages.items()) == {
        "my-plugin",
        "some-library",
        "maya",
    }
    assert packages.get("maya").spec.pkg.version == "2019.0.0"


def test_solver_dependency_reiterate(solver: Union[Solver, legacy.Solver]) -> None:

    # test what happens when a package iterator must be run through twice
    # - walking back up the solve graph should reset the iterator to where it was

    repo = make_repo(
        [
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "some-library/1"}]},
            },
            {"pkg": "maya/2019.2.0"},
            {"pkg": "maya/2019.0.0"},
            # asking for a maya version that doesn't exist will run out the iterator
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2018.0.0"}]},
            },
            # the second attempt at some-library will find maya 2019 properly
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2019.0.0"}]},
            },
        ]
    )
    solver.add_repository(repo)
    solver.add_request("my-plugin")
    try:
        packages = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert set(s.spec.pkg.name for s in packages.items()) == {
        "my-plugin",
        "some-library",
        "maya",
    }
    assert packages.get("maya").spec.pkg.version == "2019.0.0"


def test_solver_dependency_reopen_unsolvable(
    solver: Union[Solver, legacy.Solver]
) -> None:

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
    solver.add_repository(repo)
    solver.add_request("pkg-top")
    with pytest.raises(SolverError):
        packages = solver.solve()
        print(packages)


def test_solver_pre_release_config(solver: Union[Solver, legacy.Solver]) -> None:

    repo = make_repo(
        [
            {"pkg": "my-pkg/0.9.0"},
            {"pkg": "my-pkg/1.0.0-pre.0"},
            {"pkg": "my-pkg/1.0.0-pre.1"},
            {"pkg": "my-pkg/1.0.0-pre.2"},
        ]
    )

    solver.add_repository(repo)
    solver.add_request("my-pkg")

    solution = solver.solve()
    assert (
        solution.get("my-pkg").spec.pkg.version == "0.9.0"
    ), "should not resolve pre-release by default"

    solver.reset()
    solver.add_repository(repo)
    solver.add_request(
        api.request_from_dict({"pkg": "my-pkg", "prereleasePolicy": "IncludeAll"})
    )

    solution = solver.solve()
    assert solution.get("my-pkg").spec.pkg.version == "1.0.0-pre.2"


def test_solver_constraint_only(solver: Union[Solver, legacy.Solver]) -> None:

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
            }
        ]
    )
    solver.add_repository(repo)
    solver.add_request("vnp3")
    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    with pytest.raises(KeyError):
        solution.get("python")


def test_solver_constraint_and_request(solver: Union[Solver, legacy.Solver]) -> None:

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
    solver.add_repository(repo)
    solver.add_request("my-tool")
    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("python").spec.pkg.version == "3.7.3"


def test_solver_option_compatibility(solver: Union[Solver, legacy.Solver]) -> None:

    # test what happens when an option is given in the solver
    # - the options for each build are checked
    # - the resolved build must have used the option

    spec = api.Spec.from_dict(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                # favoritize 2.7, otherwise an option of python=2 doesn't actually
                # exclude python 3 from being resolved
                "options": [{"pkg": "python/~2.7"}],
                "variants": [{"python": "3.7"}, {"python": "2.7"}],
            },
        }
    )
    print(
        make_build(spec.to_dict(), [make_build({"pkg": "python/2.7.5"})])
        .build.options[0]
        .get_value()
    )
    repo = make_repo(
        [
            make_build(spec.to_dict(), [make_build({"pkg": "python/2.7.5"})]),
            make_build(spec.to_dict(), [make_build({"pkg": "python/3.7.3"})]),
        ]
    )
    repo.publish_spec(spec)

    for pyver in ("2", "2.7", "2.7.5", "3", "3.7", "3.7.3"):
        solver.reset()
        solver.add_request(api.VarRequest("python", pyver))
        solver.add_repository(repo)
        solver.add_request("vnp3")
        try:
            solution = solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))

        resolved = solution.get("vnp3")
        opt = resolved.spec.build.options[0]
        value = opt.get_value()
        assert value is not None
        assert value.startswith(f"~{pyver}"), f"{value} should start with ~{pyver}"


def test_solver_option_injection() -> None:

    # test the options that are defined when a package is resolved
    # - options are namespaced and added to the environment

    spec = api.Spec.from_dict(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                "options": [
                    {"pkg": "python"},
                    {"var": "python.abi/cp27mu"},
                    {"var": "debug/on"},
                    {"var": "special"},
                ],
            },
        }
    )
    pybuild = make_build(
        {
            "pkg": "python/2.7.5",
            "build": {"options": [{"var": "abi/cp27mu"}]},
        }
    )
    repo = make_repo([make_build(spec.to_dict(), [pybuild])])
    repo.publish_spec(spec)

    solver = Solver()
    solver.add_repository(repo)
    solver.add_request("vnp3")
    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    opts = solution.options()
    assert opts["vnp3"] == "~2.0.0"
    assert opts["vnp3.python"] == "~2.7.5"
    assert opts["vnp3.debug"] == "on"
    assert opts["python.abi"] == "cp27mu"
    assert "vnp3.special" not in opts, "should not define empty values"
    assert len(opts) == 4, "expected no more options"


def test_solver_build_from_source() -> None:

    # test when no appropriate build exists but the source is available
    # - the build is skipped
    # - the source package is checked for current options
    # - a new build is created
    # - the local package is used in the resolve

    repo = make_repo(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
        api.OptionMap(debug="off"),
    )

    solver = Solver()
    solver.add_repository(repo)
    # the new option value should disqulify the existing build
    # but a new one should be generated for this set of options
    solver.add_request(api.VarRequest("debug", "on"))
    solver.add_request("my-tool")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    resolved = solution.get("my-tool")
    assert (
        resolved.is_source_build()
    ), f"Should set unbuilt spec as source: {resolved.spec.pkg}"

    solver.reset()
    solver.add_repository(repo)
    solver.add_request(api.VarRequest("debug", "on"))
    solver.add_request("my-tool")
    solver.set_binary_only(True)
    with pytest.raises(SolverError):
        # Should fail when binary-only is specified
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))


def test_solver_build_from_source_unsolvable(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when no appropriate build exists but the source is available
    # - if the requested pkg cannot resolve a build environment
    # - this is flagged by the solver as impossible

    gcc48 = make_build({"pkg": "gcc/4.8"})
    repo = make_repo(
        [
            gcc48,
            make_build(
                {
                    "pkg": "my-tool/1.2.0",
                    "build": {"options": [{"pkg": "gcc"}], "script": "echo BUILD"},
                },
                [gcc48],
            ),
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"pkg": "gcc"}], "script": "echo BUILD"},
            },
        ],
        api.OptionMap(gcc="4.8"),
    )

    solver.add_repository(repo)
    # the new option value should disqualify the existing build
    # and there is no 6.3 that can be resolved for this request
    solver.add_request(api.VarRequest("gcc", "6.3"))
    solver.add_request("my-tool")

    with pytest.raises(SolverError):
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))


def test_solver_build_from_source_dependency() -> None:

    # test when no appropriate build exists but the source is available
    # - the existing build is skipped
    # - the source package is checked for current options
    # - a new build is created of the dependent
    # - the local package is used in the resolve

    python36 = make_build({"pkg": "python/3.6.3", "compat": "x.a.b"})
    build_with_py36 = make_build(
        {
            "pkg": "my-tool/1.2.0",
            "build": {"options": [{"pkg": "python"}]},
            "install": {"requirements": [{"pkg": "python/3.6.3"}]},
        },
        [python36],
    )

    repo = make_repo(
        [
            # the source package pins the build environment package
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"pkg": "python"}]},
                "install": {
                    "requirements": [{"pkg": "python", "fromBuildEnv": "x.x.x"}]
                },
            },
            # one existing build exists that used python 3.6.3
            build_with_py36,
            # only python 3.7 exists, which is api compatible, but not abi
            {"pkg": "python/3.7.3", "compat": "x.a.b"},
        ],
    )

    solver = Solver()
    # the new option value should disqulify the existing build
    # but a new one should be generated for this set of options
    solver.update_options(api.OptionMap(debug="on"))
    solver.add_repository(repo)
    solver.add_request("my-tool")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("my-tool").is_source_build(), "should want to build"


def test_solver_deprecated_build(solver: Union[Solver, legacy.Solver]) -> None:

    specs = [{"pkg": "my-pkg/0.9.0"}, {"pkg": "my-pkg/1.0.0"}]
    deprecated = make_build({"pkg": "my-pkg/1.0.0", "deprecated": True})
    repo = make_repo([*specs, deprecated])

    solver.add_repository(repo)
    solver.add_request("my-pkg")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert (
        solution.get("my-pkg").spec.pkg.version == "0.9.0"
    ), "should not resolve deprecated build by default"

    solver.reset()
    solver.add_repository(repo)
    solver.add_request(api.request_from_dict({"pkg": str(deprecated.pkg)}))

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert (
        solution.get("my-pkg").spec.pkg.version == "1.0.0"
    ), "should be able to resolve exact deprecated build"


def test_solver_deprecated_version(solver: Union[Solver, legacy.Solver]) -> None:

    specs = [{"pkg": "my-pkg/0.9.0"}, {"pkg": "my-pkg/1.0.0", "deprecated": True}]
    deprecated = make_build({"pkg": "my-pkg/1.0.0"})
    deprecated.deprecated = True
    repo = make_repo(specs + [deprecated])  # type: ignore

    solver.add_repository(repo)
    solver.add_request("my-pkg")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert (
        solution.get("my-pkg").spec.pkg.version == "0.9.0"
    ), "should not resolve build when version is deprecated by default"

    solver.reset()
    solver.add_repository(repo)
    solver.add_request(api.request_from_dict({"pkg": str(deprecated.pkg)}))

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))
    assert (
        solution.get("my-pkg").spec.pkg.version == "1.0.0"
    ), "should be able to resolve exact build when version is deprecated"


def test_solver_build_from_source_deprecated(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when no appropriate build exists and the main package
    # has been deprecated, no source build should be allowed

    repo = make_repo(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
        api.OptionMap(debug="off"),
    )
    repo._specs["my-tool"]["1.2.0"].deprecated = True

    solver.add_repository(repo)
    solver.add_request(api.VarRequest("debug", "on"))
    solver.add_request("my-tool")

    with pytest.raises(SolverError):
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))


def test_solver_embedded_package_adds_request(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when there is an embedded package
    # - the embedded package is added to the solution
    # - the embedded package is also added as a request in the resolve

    repo = make_repo(
        [
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
        ]
    )

    solver.add_repository(repo)
    solver.add_request("maya")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("qt").request.pkg.build == api.EMBEDDED
    assert solution.get("qt").spec.pkg.version == "5.12.6"
    assert solution.get("qt").spec.pkg.build == api.EMBEDDED


def test_solver_embedded_package_solvable(solver: Union[Solver, legacy.Solver]) -> None:

    # test when there is an embedded package
    # - the embedded package is added to the solution
    # - the embedded package resolves existing requests
    # - the solution includes the embedded packages

    repo = make_repo(
        [
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.13.0",
                "build": {"script": "echo BUILD"},
            },
        ]
    )

    solver.add_repository(repo)
    solver.add_request("qt")
    solver.add_request("maya")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("qt").spec.pkg.version == "5.12.6"
    assert solution.get("qt").spec.pkg.build == api.EMBEDDED


def test_solver_embedded_package_unsolvable(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when there is an embedded package
    # - the embedded package is added to the solution
    # - the embedded package conflicts with existing requests

    repo = make_repo(
        [
            {
                "pkg": "my-plugin",
                # the qt/5.13 requirement is available but conflits with maya embedded
                "install": {"requirements": [{"pkg": "maya/2019"}, {"pkg": "qt/5.13"}]},
            },
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.13.0",
                "build": {"script": "echo BUILD"},
            },
        ]
    )

    solver.add_repository(repo)
    solver.add_request("my-plugin")

    with pytest.raises(SolverError):
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))


def test_solver_some_versions_conflicting_requests(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when there is a package with some version that have a conflicting dependency
    # - the solver passes over the one with conflicting
    # - tthe solver logs compat info for versions with conflicts

    repo = make_repo(
        [
            {
                "pkg": "my-lib",
                "install": {
                    # python 2.7 requirement will conflict with the first (2.1) build of dep
                    "requirements": [{"pkg": "python/=2.7.5"}, {"pkg": "dep/2"}]
                },
            },
            {
                "pkg": "dep/2.1.0",
                "install": {"requirements": [{"pkg": "python/=3.7.3"}]},
            },
            {
                "pkg": "dep/2.0.0",
                "install": {"requirements": [{"pkg": "python/=2.7.5"}]},
            },
            {"pkg": "python/2.7.5"},
            {"pkg": "python/3.7.3"},
        ]
    )

    solver.add_repository(repo)
    solver.add_request("my-lib")

    try:
        solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))


def test_solver_embedded_request_invalidates(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when a package is resolved with an incompatible embedded pkg
    # - the solver tries to resolve the package
    # - there is a conflict in the embedded request

    repo = make_repo(
        [
            {
                "pkg": "my-lib",
                "install": {
                    # python 2.7 requirement will conflict with the maya embedded one
                    "requirements": [{"pkg": "python/3.7"}, {"pkg": "maya/2020"}]
                },
            },
            {
                "pkg": "maya/2020",
                "install": {"embedded": [{"pkg": "python/2.7.5"}]},
            },
            {"pkg": "python/2.7.5"},
            {"pkg": "python/3.7.3"},
        ]
    )

    solver.add_repository(repo)
    solver.add_request("python")
    solver.add_request("my-lib")

    with pytest.raises(SolverError):
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))


def test_solver_unknown_package_options(solver: Union[Solver, legacy.Solver]) -> None:

    # test when a package is requested with specific options (eg: pkg.opt)
    # - the solver ignores versions that don't define the option
    # - the solver resolves versions that do define the option

    repo = make_repo([{"pkg": "my-lib/2.0.0"}])
    solver.add_repository(repo)

    # this option is specific to the my-lib package and is not known by the package
    solver.add_request(api.VarRequest("my-lib.something", "value"))
    solver.add_request("my-lib")

    with pytest.raises(SolverError):
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))

    # this time we don't request that option, and it should be ok
    solver.reset()
    solver.add_repository(repo)
    solver.add_request("my-lib")
    try:
        solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))


def test_solver_var_requirements(solver: Union[Solver, legacy.Solver]) -> None:

    # test what happens when a dependency is added which is incompatible
    # with an existing request in the stack
    repo = make_repo(
        [
            {
                "pkg": "python/2.7.5",
                "build": {"options": [{"var": "abi", "static": "cp27mu"}]},
            },
            {
                "pkg": "python/3.7.3",
                "build": {"options": [{"var": "abi", "static": "cp37m"}]},
            },
            {
                "pkg": "my-app/1.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp27mu"}]
                },
            },
            {
                "pkg": "my-app/2.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp37m"}]
                },
            },
        ]
    )

    solver.add_repository(repo)
    solver.add_request("my-app/2")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("my-app").spec.pkg.version == "2.0.0"
    assert solution.get("python").spec.pkg.version == "3.7.3"

    # requesting the older version of my-app should force old python abi
    solver.reset()
    solver.add_repository(repo)
    solver.add_request("my-app/1")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("python").spec.pkg.version == "2.7.5"


def test_solver_var_requirements_unresolve(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when a package is resolved that conflicts in var requirements
    #  - the solver should unresolve the solved package
    #  - the solver should resolve a new version of the package with the right version
    repo = make_repo(
        [
            {
                "pkg": "python/2.7.5",
                "build": {"options": [{"var": "abi", "static": "cp27"}]},
            },
            {
                "pkg": "python/3.7.3",
                "build": {"options": [{"var": "abi", "static": "cp37"}]},
            },
            {
                "pkg": "my-app/1.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp27"}]
                },
            },
            {
                "pkg": "my-app/2.0.0",
                "install": {"requirements": [{"pkg": "python"}, {"var": "abi/cp27"}]},
            },
        ]
    )

    solver.add_repository(repo)
    # python is resolved first to get 3.7
    solver.add_request("python")
    # the addition of this app constrains the python.abi to 2.7
    solver.add_request("my-app/1")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("my-app").spec.pkg.version == "1.0.0"
    assert (
        solution.get("python").spec.pkg.version == "2.7.5"
    ), "should re-resolve python"

    solver.reset()
    solver.add_repository(repo)
    # python is resolved first to get 3.7
    solver.add_request("python")
    # the addition of this app constrains the global abi to 2.7
    solver.add_request("my-app/2")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert solution.get("my-app").spec.pkg.version == "2.0.0"
    assert (
        solution.get("python").spec.pkg.version == "2.7.5"
    ), "should re-resolve python"


def test_solver_build_options_dont_affect_compat(
    solver: Union[Solver, legacy.Solver]
) -> None:

    # test when a package is resolved with some build option
    #  - that option can conflict with another packages build options
    #  - as long as there is no explicit requirement on that option's value

    dep_v1 = api.Spec.from_dict({"pkg": "build-dep/1.0.0"})
    dep_v2 = api.Spec.from_dict({"pkg": "build-dep/2.0.0"})

    a_spec = {
        "pkg": "pkga/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=1.0.0"}, {"var": "debug/on"}]},
    }

    b_spec = {
        "pkg": "pkgb/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=2.0.0"}, {"var": "debug/off"}]},
    }

    repo = make_repo(
        [
            make_build(a_spec.copy(), [dep_v1]),
            make_build(b_spec.copy(), [dep_v2]),
        ]
    )
    repo.publish_spec(api.Spec.from_dict(a_spec))
    repo.publish_spec(api.Spec.from_dict(b_spec))

    solver.add_repository(repo)
    # a gets resolved and adds options for debug/on and build-dep/1
    # to the set of options in the solver
    solver.add_request("pkga")
    # b is not affected and can still be resolved
    solver.add_request("pkgb")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    solver.reset()
    solver.add_repository(repo)
    solver.add_repository(repo)
    solver.add_request("pkga")
    solver.add_request("pkgb")
    # this time the explicit request will cause a failure
    solver.add_request(api.VarRequest("build-dep", "=1.0.0"))
    with pytest.raises(SolverError):
        solution = solver.solve()
        print(io.format_resolve(solver, verbosity=100))
