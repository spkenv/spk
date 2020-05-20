import io

import spfs
import pytest

from . import api, storage
from ._nodes import BuildNode, BinaryPackageHandle
from ._solver import Solver, UnresolvedPackageError


def test_solver_no_spec(tmpspfs: spfs.storage.fs.FSRepository) -> None:

    options = api.OptionMap()

    # push just the package tag with no tag for spec/meta
    tmpspfs.tags.push_tag(
        "spk/pkg/my_pkg/1.0.0/" + options.digest(), spfs.encoding.EMPTY_DIGEST
    )

    solver = Solver(options)
    solver.add_request("my_pkg")

    with pytest.raises(UnresolvedPackageError):
        nodes = solver.solve()


def test_solver_existing_tag(tmprepo: storage.SpFSRepository) -> None:

    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))

    tmprepo.publish_spec(spec)
    tmprepo.publish_package(spec.pkg, options, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_request(api.parse_ident("my_pkg"))

    env = solver.solve()
    assert len(env.inputs) == 1, "expected one resolved package"
    source = env.inputs["my_pkg"].follow()
    assert isinstance(source, BuildNode)
    # TODO: assert that it does not need building


def test_solver_source_only(tmprepo: storage.SpFSRepository) -> None:

    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))
    tmprepo.publish_spec(spec)
    tmprepo.publish_source_package(spec.pkg, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_request(api.parse_ident("my_pkg"))

    env = solver.solve()
    assert len(env.inputs) == 1, "expected one resolved package"
    port = env.inputs["my_pkg"]
    assert isinstance(port.follow(), BuildNode), "expected to connect to build node"
    assert port.type is BinaryPackageHandle, "expected to provide binary package"
