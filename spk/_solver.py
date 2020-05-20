from typing import List, Union, Iterable

import structlog
import spfs

from . import graph, api, storage, compat
from ._handle import BinaryPackageHandle, SourcePackageHandle
from . import _nodes  # FIXME: circular dependency

_LOGGER = structlog.get_logger("spk")


class UnresolvedPackageError(RuntimeError):
    def __init__(self, pkg: str, versions: List[str] = None) -> None:

        message = f"{pkg}"
        if versions is not None:
            version_list = "\n".join(versions)
            message += f" - from versions: [{version_list}]"
        super(UnresolvedPackageError, self).__init__(message)


class Env(graph.Node):
    def run(self) -> None:

        pass

    def packages(self) -> Iterable[BinaryPackageHandle]:

        for name, port in self.inputs.items():

            if port.value is None:
                raise RuntimeError(
                    f"Package has not been built for this environment: {name}"
                )

            if not isinstance(port.value, _nodes.BinaryPackageHandle):
                _LOGGER.warning(
                    f"Unexpected input port on resolved environment: {name} ({port.type})"
                )

            yield port.value


class Solver:
    def __init__(self, options: api.OptionMap) -> None:

        self._repos: List[storage.Repository] = []
        self._options = options
        self._requests: List[api.Ident] = []
        self._specs: List[api.Spec] = []

    def add_repository(self, repo: storage.Repository) -> None:

        self._repos.append(repo)

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        self._requests.append(pkg)

    def add_spec(self, spec: api.Spec) -> None:

        self._specs.append(spec)

    def solve(self) -> Env:

        # TODO: FIXME: support many repos
        assert len(self._repos) == 1
        repo = self._repos[0]

        env = Env()
        for request in self._requests:

            all_versions = repo.list_package_versions(request.name)
            all_versions.sort()
            versions = list(filter(request.version.is_satisfied_by, all_versions))
            versions.sort()

            for version_str in reversed(versions):

                version = compat.parse_version(version_str)
                pkg = api.Ident(request.name, version)
                spec = repo.read_spec(pkg)
                options = spec.resolve_all_options(self._options)

                try:
                    digest = repo.get_package(pkg, options)
                except storage.PackageNotFoundError:
                    _LOGGER.debug(
                        "package not built with required options:", pkg=pkg, **options
                    )
                    pass
                else:
                    builder = _nodes.BuildNode(spec, options)
                    builder.binary_package.value = BinaryPackageHandle(
                        spec, digest.str()
                    )
                    port = env.add_input_port(pkg.name, _nodes.BinaryPackageHandle)
                    port.connect(builder.binary_package)
                    break

                try:
                    digest = repo.get_source_package(api.Ident(request.name, version))
                except storage.PackageNotFoundError:
                    pass
                else:
                    builder = _nodes.BuildNode(spec, options)
                    # builder.source_package.value = SourcePackageHandle(
                    #     spec, digest.str()
                    # )
                    port = env.add_input_port(pkg.name, _nodes.BinaryPackageHandle)
                    port.connect(builder.binary_package)
                    break

                # TODO: try for source package
                # tag = f"spk/pkg/{pkg.name}/{version}/{options.digest()}"

                # if repo.tags.has_tag(tag):
                #     nodes.append(BinaryPackageNode(spec, tag)))
                #     break
                # else:
                #     nodes.append(SpecBuilder(spec, options))
                #     break
            else:
                raise UnresolvedPackageError(str(request), versions=all_versions)

        for spec in self._specs:
            # FIXME: combine with requeests more elegantly
            options = spec.resolve_all_options(self._options)
            builder = _nodes.BuildNode(spec, options)
            fetcher = _nodes.FetchNode(spec)
            builder.source_package.connect(fetcher.source_package)
            port = env.add_input_port(spec.pkg.name, _nodes.BinaryPackageHandle)
            port.connect(builder.binary_package)

        return env
