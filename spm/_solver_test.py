import io

import spfs
import pytest

from . import api, storage
from ._nodes import BinaryPackageNode
from ._solver import Solver, UnresolvedPackageError


def test_solver_no_spec(tmpspfs: spfs.storage.fs.FSRepository) -> None:

    options = api.OptionMap()

    # push just the package tag with no tag for spec/meta
    tmpspfs.tags.push_tag(
        "spm/pkg/my_pkg/1.0.0/" + options.digest(), spfs.encoding.EMPTY_DIGEST
    )

    solver = Solver(options)
    solver.add_request("my_pkg")

    with pytest.raises(UnresolvedPackageError):
        nodes = solver.solve()


def test_solver_existing_tag(tmprepo: storage.SpFSRepository) -> None:

    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))

    # FIXME: these functions don't read consistently
    tmprepo.publish_spec(spec)
    tmprepo.publish_package(spec.pkg, options, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_request(api.parse_ident("my_pkg"))

    nodes = solver.solve()
    assert len(nodes) == 1, "expected one resolved node"
    assert isinstance(nodes[0], BinaryPackageNode), "expected to resolve binary package"
    assert len(list(nodes[0].inputs())) == 0, "expected no inputs to need resolving"


def test_solver_source_only(tmprepo: storage.SpFSRepository) -> None:

    options = api.OptionMap()
    spec = api.Spec(api.parse_ident("my_pkg/1.0.0"))
    tmprepo.publish_spec(spec)
    tmprepo.publish_source_package(spec.pkg, spfs.encoding.EMPTY_DIGEST)

    solver = Solver(options)
    solver.add_request(api.parse_ident("my_pkg"))

    nodes = solver.solve()
    assert len(nodes) == 1, "expected one resolved node"
    assert isinstance(nodes[0], BinaryPackageNode), "expected to resolve binary package"
    inputs = list(nodes[0].inputs())
    assert len(inputs) == 1, "expected binary build inputs to need resolving"
    # assert isinstance(nodes[0], Spec), "expected to resolve binary package"
