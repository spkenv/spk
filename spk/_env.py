import os

from . import api, build, solve


ACTIVE_PREFIX = os.getenv("SPK_ACTIVE_PREFIX", "/spfs")


class NoEnvironmentError(RuntimeError):
    """Denotes that an active environment was required, but does not exist."""

    def __init__(self) -> None:
        super(NoEnvironmentError, self).__init__("Not running in an spk environment")


def current_env() -> solve.Solution:
    """Load the current environment from the spfs file system."""

    metadata_dir = build.data_path(prefix=ACTIVE_PREFIX)
    try:
        package_names = os.listdir(metadata_dir)
    except FileNotFoundError:
        raise NoEnvironmentError()

    solution = solve.Solution()
    for name in package_names:
        versions = os.listdir(os.path.join(metadata_dir, name))
        for version in versions:
            builds = os.listdir(os.path.join(metadata_dir, name, version))
            for digest in builds:

                pkg = api.parse_ident(f"{name}/{version}/{digest}")
                spec = _read_installed_spec(pkg)
                request = api.Request(
                    api.parse_ident_range(f"{name}/={version}/{digest}"),
                    prerelease_policy=api.PreReleasePolicy.IncludeAll,
                )
                solution.add(request, spec, None)

    return solution


def _read_installed_spec(pkg: api.Ident) -> api.Spec:

    path = build.build_spec_path(pkg, ACTIVE_PREFIX)
    return api.read_spec_file(path)


def load_env(name: str = "default", filename: str = "./.spk-env.yaml") -> api.Env:
    """Load a named environment from an environment spec file."""

    env_spec = api.read_env_spec_file(filename)
    return env_spec.get_env(name)
