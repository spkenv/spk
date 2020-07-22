import os

from . import api, build, solve, storage


ACTIVE_PREFIX = os.getenv("SPK_ACTIVE_PREFIX", "/spfs")


class NoEnvironmentError(RuntimeError):
    """Denotes that an active environment was required, but does not exist."""

    def __init__(self) -> None:
        super(NoEnvironmentError, self).__init__("Not running in an spk environment")


def current_env() -> solve.Solution:
    """Load the current environment from the spfs file system."""

    runtime = storage.RuntimeRepository()
    solution = solve.Solution()
    for name in runtime.list_packages():
        for version in runtime.list_package_versions(name):
            for pkg in runtime.list_package_builds(name + "/" + version):

                spec = runtime.read_spec(pkg)
                request = api.Request(
                    api.parse_ident_range(f"{pkg.name}/={pkg.version}/{pkg.build}"),
                    prerelease_policy=api.PreReleasePolicy.IncludeAll,
                )
                solution.add(request, spec, runtime)

    return solution


def load_env(name: str = "default", filename: str = "./.spk-env.yaml") -> api.Env:
    """Load a named environment from an environment spec file."""

    env_spec = api.read_env_spec_file(filename)
    return env_spec.get_env(name)
