from typing import List
import os
import stat
import subprocess

import structlog
import spfs

from .. import api, storage, solve, exec
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

    runtime = spfs.active_runtime()
    repo = storage.local_repository()
    solver = solve.Solver(options)
    # FIXME: allow this to be configurable on the command line
    solver.add_repository(storage.remote_repository())
    solver.add_repository(repo)
    for opt in spec.opts:
        if not isinstance(opt, api.Request):
            continue
        if opt.pkg.name in options:
            opt = opt.clone()
            opt.pkg.version = api.parse_version_range(options[opt.pkg.name])
        solver.add_request(opt)

    solution = solver.solve()
    exec.configure_runtime(runtime, solution)
    runtime.set_editable(True)
    spfs.remount_runtime(runtime)

    os.environ.update(solution.to_environment())
    layer = build_and_commit_artifacts(spec, sources, options)
    pkg = spec.pkg.with_build(options.digest())
    repo.publish_package(pkg, layer.digest())
    return pkg


def build_and_commit_artifacts(
    spec: api.Spec, sources: str, options: api.OptionMap
) -> spfs.storage.Layer:

    prefix = "/spfs"
    build_artifacts(spec, sources, options, prefix)

    diffs = spfs.diff()
    validate_build_changeset(diffs, prefix)

    runtime = spfs.active_runtime()
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
    env.update(options.to_environment())
    env["PREFIX"] = prefix

    proc = subprocess.Popen(["/bin/sh", "-ex", build_script], cwd=sources, env=env)
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
        if diff.entries:
            a, b = diff.entries
            if stat.S_ISDIR(a.mode) and stat.S_ISDIR(b.mode):
                continue
        if diff.mode is not spfs.tracking.DiffMode.added:
            raise BuildError(f"Existing file was modified: {prefix}{diff.path}")
