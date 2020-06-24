from typing import List, Iterable, Optional, MutableMapping
import os
import stat
import json
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
    ...         "pkg": "my-pkg",
    ...         "build": {"script": "echo hello, world"},
    ...      }))
    ...     .with_option("debug", "true")
    ...     .build()
    ... )
    my-pkg/3I42H3S6
    """

    def __init__(self) -> None:

        self._spec: Optional[api.Spec] = None
        self._options: api.OptionMap = api.OptionMap()
        self._source_dir: str = "."
        self._repos: List[storage.Repository] = []

    @staticmethod
    def from_spec(spec: api.Spec) -> "BinaryPackageBuilder":

        builder = BinaryPackageBuilder()
        builder._spec = spec.clone()
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

        for opt in self._spec.build.options:
            if not isinstance(opt, api.PkgOpt):
                continue
            request = opt.to_request(build_options.get(opt.pkg))
            build_env_solver.add_request(request)

        solution = build_env_solver.solve()
        exec.configure_runtime(runtime, solution)
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)

        self._spec.render_all_pins(s for _, s, _ in solution.items())
        layer = build_and_commit_artifacts(
            self._spec, self._source_dir, build_options, solution.to_environment()
        )
        pkg = self._spec.pkg.with_build(build_options.digest())
        spec = self._spec.clone()
        spec.pkg = pkg
        storage.local_repository().publish_package(spec, layer.digest())
        return pkg


def build_and_commit_artifacts(
    spec: api.Spec, sources: str, options: api.OptionMap, env: MutableMapping[str, str]
) -> spfs.storage.Layer:

    prefix = "/spfs"
    build_artifacts(spec, sources, options, prefix, env)

    diffs = spfs.diff()
    validate_build_changeset(diffs, prefix)

    runtime = spfs.active_runtime()
    return spfs.commit_layer(runtime)


def build_artifacts(
    spec: api.Spec,
    sources: str,
    options: api.OptionMap,
    prefix: str,
    env: MutableMapping[str, str] = None,
) -> None:

    pkg = spec.pkg.with_build(options.digest())

    os.makedirs(prefix, exist_ok=True)

    metadata_dir = data_path(pkg, prefix=prefix)
    build_options = build_options_path(pkg, prefix=prefix)
    build_script = build_script_path(pkg, prefix=prefix)
    os.makedirs(metadata_dir, exist_ok=True)
    with open(build_script, "w+") as writer:
        writer.write(spec.build.script)
    with open(build_options, "w+") as writer:
        json.dump(options, writer, indent="\t")

    env = env or {}
    env.update(options.to_environment())
    env["PREFIX"] = prefix

    proc = subprocess.Popen(["/bin/sh", "-ex", build_script], cwd=sources, env=env)
    proc.wait()
    if proc.returncode != 0:
        raise BuildError(
            f"Build script returned non-zero exit status: {proc.returncode}"
        )


def build_options_path(pkg: api.Ident, prefix: str = "/spfs") -> str:
    """Return the file path for the given build's options.json file.

    This file is created during a build and stores the set
    of build options used when creating the package
    """
    return os.path.join(data_path(pkg, prefix), "options.json")


def build_script_path(pkg: api.Ident, prefix: str = "/spfs") -> str:
    """Return the file path for the given build's build.sh file.

    This file is created during a build and stores the bash
    script used to build the package contents
    """
    return os.path.join(data_path(pkg, prefix), "build.sh")


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
