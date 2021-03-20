import os

import spkrs
import structlog

from . import api, solve, storage

_LOGGER = structlog.get_logger("spk")
ACTIVE_PREFIX = os.getenv("SPK_ACTIVE_PREFIX", "/spfs")
ENV_FILENAME = ".spk-env.yaml"


class NoEnvironmentError(RuntimeError):
    """Denotes that an active environment was required, but does not exist."""

    def __init__(self) -> None:
        super(NoEnvironmentError, self).__init__("Not running in an spk environment")


def current_env() -> solve.Solution:
    """Load the current environment from the spfs file system."""

    try:
        spkrs.active_runtime()
    except RuntimeError:
        raise NoEnvironmentError()

    runtime = storage.RuntimeRepository()
    solution = solve.Solution()
    for name in runtime.list_packages():
        for version in runtime.list_package_versions(name):
            for pkg in runtime.list_package_builds(name + "/" + version):

                spec = runtime.read_spec(pkg)
                request = api.PkgRequest(
                    api.parse_ident_range(f"{pkg.name}/={pkg.version}/{pkg.build}"),
                    prerelease_policy=api.PreReleasePolicy.IncludeAll,
                )
                solution.add(request, spec, runtime)

    return solution
