import io

import spfs
import pytest

from . import api, storage
from ._nodes import BuildNode, BinaryPackageHandle
from ._solver import Solver, UnresolvedPackageError


def test_solver_no_spec() -> None:

    repo = storage.MemRepository()

    pkg = api.parse_ident("my_pkg/1.0.0")
    options = api.OptionMap()

    # publish package without publishing spec
    repo.publish_package(pkg, options, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request("my_pkg")

    with pytest.raises(UnresolvedPackageError):
        solver.solve()


def test_solver_existing_tag() -> None:

    repo = storage.MemRepository()
    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))

    repo.publish_spec(spec)
    repo.publish_package(spec.pkg, options, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request(api.parse_ident("my_pkg"))

    env = solver.solve()
    assert len(env.inputs) == 1, "expected one resolved package"
    source = env.inputs["my_pkg"].follow()
    assert isinstance(source, BuildNode)
    # TODO: assert that it does not need building


def test_solver_source_only() -> None:

    repo = storage.MemRepository()
    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))

    repo.publish_spec(spec)
    repo.publish_source_package(spec.pkg, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_repository(repo)
    solver.add_request(api.parse_ident("my_pkg"))

    env = solver.solve()
    assert len(env.inputs) == 1, "expected one resolved package"
    port = env.inputs["my_pkg"]
    assert isinstance(port.follow(), BuildNode), "expected to connect to build node"
    assert port.type is BinaryPackageHandle, "expected to provide binary package"
