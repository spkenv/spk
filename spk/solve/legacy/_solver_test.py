import pytest

from ... import api, io
from .._solver_test import make_repo
from ._solver import Solver, SolverError


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

    solver = Solver({})
    # the new option value should disqulify the existing build
    # but a new one should be generated for this set of options
    solver.update_options(api.OptionMap(debug="on"))
    solver.add_repository(repo)
    solver.add_request("my-tool")

    try:
        solution = solver.solve()
    finally:
        print(io.format_resolve(solver, verbosity=100))

    assert (
        solution.get("my-tool").spec.pkg.build is None
    ), "Should set unbuilt spec as source"

    solver.reset()
    solver.update_options(api.OptionMap(debug="on"))
    solver.add_repository(repo)
    solver.add_request("my-tool")
    solver.set_binary_only(True)
    with pytest.raises(SolverError):
        # Should fail when binary-only is specified
        try:
            solver.solve()
        finally:
            print(io.format_resolve(solver, verbosity=100))
