import io

import spfs
import pytest

from . import api, storage
from ._nodes import BuildNode, BinaryPackageHandle
from ._solver import Solver, UnresolvedPackageError


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
