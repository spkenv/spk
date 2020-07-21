import os

from . import api, build


ACTIVE_PREFIX = os.getenv("SPK_ACTIVE_PREFIX", "/spfs")


class NoEnvironmentError(RuntimeError):
    """Denotes that an active environment was required, but does not exist."""

    def __init__(self) -> None:
        super(NoEnvironmentError, self).__init__("Not running in an spk environment")


def current_env() -> api.Env:
    """Load the current environment from the spfs file system."""

    metadata_dir = build.data_path(prefix=ACTIVE_PREFIX)
    try:
        package_names = os.listdir(metadata_dir)
    except FileNotFoundError:
        raise NoEnvironmentError()

    env = api.Env("installed")
    for name in package_names:
        versions = os.listdir(os.path.join(metadata_dir, name))
        for version in versions:
            builds = os.listdir(os.path.join(metadata_dir, name, version))
            for build in builds:
                request = f"{name}/={version}/{build}"
                env.requirements.append(
                    api.Request(
                        api.parse_ident_range(request),
                        prerelease_policy=api.PreReleasePolicy.IncludeAll,
                    )
                )

    return env


def load_env(name: str = "default", filename: str = "./.spk-env.yaml") -> api.Env:
    """Load a named environment from an environment spec file."""

    env_spec = api.read_env_spec_file(filename)
    return env_spec.get_env(name)
