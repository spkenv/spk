from typing import List, Union

import structlog
import spfs

from . import graph, api, storage
from ._nodes import BinaryPackageNode, SourcePackageNode
from ._handle import Handle, SpFSHandle

_LOGGER = structlog.get_logger("spm")


class UnresolvedPackageError(RuntimeError):
    def __init__(self, pkg: str, versions: List[str] = None) -> None:

        message = f"{pkg}"
        if versions:
            message += " - from versions: " + "\n".join(versions)
        super(UnresolvedPackageError, self).__init__(message)


class Solver:
    def __init__(self, options: api.OptionMap) -> None:

        self._options = options
        self._requests: List[api.Ident] = []

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        self._requests.append(pkg)

    def solve(self) -> List[graph.Node]:

        # TODO: not this, something more elegant?
        repo = storage.SpFSRepository(spfs.get_config().get_repository())

        nodes: List[graph.Node] = []
        for pkg in self._requests:

            all_versions = repo.list_package_versions(pkg.name)
            all_versions.sort()
            versions = list(filter(pkg.version.is_satisfied_by, all_versions))
            versions.sort()

            for version in reversed(versions):

                spec = repo.read_spec(api.Ident(pkg.name, api.parse_version(version)))
                options = spec.resolve_all_options(self._options)

                try:
                    digest = repo.resolve_package(
                        api.Ident(pkg.name, api.parse_version(version)), options
                    )
                except storage.UnknownPackageError:
                    pass
                else:
                    nodes.append(BinaryPackageNode(SpFSHandle(spec, digest.str())))
                    break

                try:
                    digest = repo.resolve_source_package(
                        api.Ident(pkg.name, api.parse_version(version))
                    )
                except storage.UnknownPackageError:
                    pass
                else:
                    builder = BuildNode(spec, options, digest)
                    nodes.append(BinaryPackageNode(builder))
                    break

                # TODO: try for source package
                # tag = f"spm/pkg/{pkg.name}/{version}/{options.digest()}"

                # if repo.tags.has_tag(tag):
                #     nodes.append(BinaryPackageNode(SpFSHandle(spec, tag)))
                #     break
                # else:
                #     nodes.append(SpecBuilder(spec, options))
                #     break
            else:
                raise UnresolvedPackageError(str(pkg), versions=all_versions)

        return nodes


class BuildNode(Handle, graph.Operation):
    def __init__(
        self, spec: api.Spec, options: api.OptionMap, source: spfs.encoding.Digest
    ) -> None:

        self._spec = spec
        super(BuildNode, self).__init__()

    def spec(self) -> api.Spec:
        return self._spec

    def url(self) -> str:

        return "TODO: what is this? BUILD??? "

    def run(self) -> None:

        raise NotImplementedError("BuildNode.run")
