from typing import List
import os
import subprocess

import structlog
import spfs

from .. import api, storage
from ._env import data_path

_LOGGER = structlog.get_logger("spk.build")


class BuildError(RuntimeError):
    """Denotes an error during the build process."""

    pass


def make_binary_package(
    spec: api.Spec, sources: str, options: api.OptionMap
) -> api.Ident:
    """Build a local binary package for the given spec, source files, and options.

    The given options are not checked against the spec, and
    are expected to have been properly resolved with defaults filled in etc.
    """

    spfs_repo = spfs.get_config().get_repository()
    repo = storage.SpFSRepository(spfs_repo)
    layer = build_and_commit_artifacts(spec, sources, options)
    pkg = spec.pkg.with_build(options.digest())
    repo.publish_package(pkg, layer.digest())
    return pkg


def build_and_commit_artifacts(
    spec: api.Spec, sources: str, options: api.OptionMap
) -> spfs.storage.Layer:

    pkg = spec.pkg.with_build(options.digest())

    runtime = spfs.active_runtime()

    prefix = "/spfs"
    build_artifacts(spec, sources, options, prefix)

    diffs = spfs.diff()
    validate_build_changeset(diffs, prefix)

    return spfs.commit_layer(runtime)


def build_artifacts(
    spec: api.Spec, sources: str, options: api.OptionMap, prefix: str
) -> None:

    pkg = spec.pkg.with_build(options.digest())

    os.makedirs(prefix, exist_ok=True)

    metadata_dir = data_path(pkg, prefix=prefix)
    build_script = os.path.join(metadata_dir, "build.sh")
    os.makedirs(metadata_dir, exist_ok=True)
    with open(build_script, "w+") as f:
        f.write(spec.build.script)

    env = os.environ.copy()
    env.update(options.to_env())
    env["PREFIX"] = prefix

    proc = subprocess.Popen(["/bin/sh", "-e", build_script], cwd=sources, env=env)
    proc.wait()
    if proc.returncode != 0:
        raise BuildError(
            f"Build script returned non-zero exit status: {proc.returncode}"
        )


def validate_build_changeset(
    diffs: List[spfs.tracking.Diff], prefix: str = "/spfs"
) -> None:

    diffs = list(
        filter(lambda diff: diff.mode is not spfs.tracking.DiffMode.unchanged, diffs)
    )

    if not diffs:
        raise BuildError(f"Build process created no files under {prefix}")

    for diff in diffs:
        _LOGGER.debug(diff)
        if diff.mode is not spfs.tracking.DiffMode.added:
            raise BuildError(f"Existing file was modified: {prefix}{diff.path}")
