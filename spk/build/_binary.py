from typing import List, Iterable, Optional
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


class BinaryPackageBuilder:
    """Builds a binary package.

    >>> (
    ...     BinaryPackageBuilder
    ...     .from_spec(api.Spec.from_dict({
    ...         "pkg": "my_pkg",
    ...         "build": {"script": "echo hello, world"},
    ...      }))
    ...     .with_option("debug", "true")
    ...     .build()
    ... )
    my_pkg/3I42H3S6
    """

    def __init__(self) -> None:

        self._spec: Optional[api.Spec] = None
        self._options: api.OptionMap = api.OptionMap()
        self._source_dir: str = "."
        self._repos: List[storage.Repository] = []

    @staticmethod
    def from_spec(spec: api.Spec) -> "BinaryPackageBuilder":

        builder = BinaryPackageBuilder()
        builder._spec = spec
        return builder

    def with_option(self, name: str, value: str) -> "BinaryPackageBuilder":

        self._options[name] = value
        return self

    def with_options(self, options: api.OptionMap) -> "BinaryPackageBuilder":

        self._options.update(options)
        return self

    def with_source_dir(self, dirname: str) -> "BinaryPackageBuilder":

        self._source_dir = os.path.abspath(dirname)
        return self

    def with_repository(self, repo: storage.Repository) -> "BinaryPackageBuilder":

        self._repos.append(repo)
        return self

    def with_repositories(
        self, repos: Iterable[storage.Repository]
    ) -> "BinaryPackageBuilder":

        self._repos.extend(repos)
        return self

    def build(self) -> api.Ident:
        """Build the requested binary package."""

        assert (
            self._spec is not None
        ), "Target spec not given, did you use SourcePackagebuilder.from_spec?"

        runtime = spfs.active_runtime()
        build_options = self._spec.resolve_all_options(self._options)

        build_env_solver = solve.Solver(self._options)
        for repo in self._repos:
            build_env_solver.add_repository(repo)

        for opt in self._spec.opts:
            if not isinstance(opt, api.Request):
                continue
            if opt.pkg.name in build_options:
                opt = opt.clone()
                opt.pkg.version = api.parse_version_range(build_options[opt.pkg.name])
            build_env_solver.add_request(opt)

        solution = build_env_solver.solve()
        exec.configure_runtime(runtime, solution)
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)

        os.environ.update(solution.to_environment())
        layer = build_and_commit_artifacts(self._spec, self._source_dir, build_options)
        pkg = self._spec.pkg.with_build(build_options.digest())
        storage.local_repository().publish_package(pkg, layer.digest())
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
