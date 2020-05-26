from typing import Iterable, Optional
import os

import spfs
import structlog

from . import graph, api, storage, build
from ._handle import BinaryPackageHandle, SourcePackageHandle

_LOGGER = structlog.get_logger("spk")


class FetchNode(graph.Node):
    """Fetch node is responsible for fetching and packaging sources."""

    def __init__(self, spec: api.Spec) -> None:

        super(FetchNode, self).__init__()
        self._spec = spec
        self.source_package = self.add_output_port(
            "source_package", SourcePackageHandle
        )

    def __str__(self) -> str:

        return f"FetchNode( pkg={self._spec.pkg} )"

    def run(self) -> None:

        build.make_source_package(self._spec)


class BuildNode(graph.Node):
    def __init__(self, spec: api.Spec, options: api.OptionMap) -> None:

        super(BuildNode, self).__init__()

        self._spec = spec
        self._options = options.copy()

        self.source_package = self.add_input_port("source_package", SourcePackageHandle)
        self.binary_package = self.add_output_port(
            "binary_package", BinaryPackageHandle
        )

    def __str__(self) -> str:

        return f"BuildNode( pkg={self._spec.pkg}/{self._options.digest()} )"

    def run(self) -> None:

        _LOGGER.info("BUILDING", pkg=self._spec.pkg)
        source = self.source_package.value
        # TODO: get location of source package sources/ set it up
        raise NotImplementedError("build node.run")
        self.binary_package.value = build.make_binary_package(
            self._spec, "", self._options
        )
