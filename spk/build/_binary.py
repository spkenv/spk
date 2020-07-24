from typing import List, Iterable, Optional, MutableMapping, Union
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
    ...     .with_source(".")
    ...     .build()
    ... )
    my-pkg/0.0.0/3I42H3S6
    """

    def __init__(self) -> None:

        self._prefix = "/spfs"
        self._spec: Optional[api.Spec] = None
        self._all_options = api.OptionMap()
        self._pkg_options = api.OptionMap()
        self._source: Union[str, api.Ident] = "."
        self._solver: Optional[solve.Solver] = None
        self._repos: List[storage.Repository] = []

    @staticmethod
    def from_spec(spec: api.Spec) -> "BinaryPackageBuilder":

        builder = BinaryPackageBuilder()
        builder._spec = spec.clone()
        builder._source = spec.pkg.with_build(api.SRC)
        return builder

    def get_build_env_decision_tree(self) -> solve.DecisionTree:
        """Return the solver decision tree for the build environment.

        This is most useful for debugging build environments that failed to resolve,
        and builds that failed with a SolverError.

        If the builder has not run, return an empty tree.
        """

        if self._solver is None:
            return solve.DecisionTree()
        return self._solver.decision_tree

    def with_option(self, name: str, value: str) -> "BinaryPackageBuilder":

        self._all_options[name] = value
        return self

    def with_options(self, options: api.OptionMap) -> "BinaryPackageBuilder":

        self._all_options.update(options)
        return self

    def with_source(self, source: Union[str, api.Ident]) -> "BinaryPackageBuilder":

        self._source = source
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
        ), "Target spec not given, did you use BinaryPackagebuilder.from_spec?"

        runtime = spfs.active_runtime()
        self._pkg_options = self._spec.resolve_all_options(self._all_options)
        self._all_options.update(self._pkg_options)

        solution = self._resolve_source_package()
        exec.configure_runtime(runtime, solution)
        solution = self._resolve_build_environment()
        exec.configure_runtime(runtime, solution)
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)

        self._spec.render_all_pins(s for _, s, _ in solution.items())
        self._spec.pkg.set_build(self._pkg_options.digest())
        for opt in self._spec.build.options:
            opt.set_value(self._pkg_options.get(opt.name(), ""))
        layer = self._build_and_commit_artifacts(solution.to_environment())
        storage.local_repository().publish_package(self._spec, layer.digest())
        return self._spec.pkg

    def _resolve_source_package(self) -> solve.Solution:

        self._solver = solve.Solver(self._all_options)
        self._solver.add_repository(storage.local_repository())

        if isinstance(self._source, api.Ident):
            ident_range = api.parse_ident_range(
                f"{self._source.name}/={self._source.version}/{self._source.build}"
            )
            request = api.Request(ident_range, api.PreReleasePolicy.IncludeAll)
            self._solver.add_request(request)

        return self._solver.solve()

    def _resolve_build_environment(self) -> solve.Solution:

        self._solver = solve.Solver(self._all_options)
        for repo in self._repos:
            self._solver.add_repository(repo)

        for request in self.get_build_requirements():
            self._solver.add_request(request)

        return self._solver.solve()

    def get_build_requirements(self) -> Iterable[api.Request]:
        """List the requirements for the build environment."""

        assert (
            self._spec is not None
        ), "Target spec not given, did you use BinaryPackagebuilder.from_spec?"

        opts = self._spec.resolve_all_options(self._all_options)
        for opt in self._spec.build.options:
            if not isinstance(opt, api.PkgOpt):
                continue
            yield opt.to_request(opts.get(opt.pkg))

    def _build_and_commit_artifacts(
        self, env: MutableMapping[str, str]
    ) -> spfs.storage.Layer:

        assert self._spec is not None, "Internal Error: spec is None"

        self._build_artifacts(env)

        sources_dir = data_path(self._spec.pkg.with_build(api.SRC), prefix=self._prefix)

        runtime = spfs.active_runtime()
        pattern = os.path.join(sources_dir[len(self._prefix) :], "**", "*")
        _LOGGER.info("Purging all changes made to source directory", dir=pattern)
        runtime.reset(pattern)
        spfs.remount_runtime(runtime)

        _LOGGER.info("Calculating package fileset...")
        diffs = spfs.diff()
        _LOGGER.info("Validating package fileset...")
        validate_build_changeset(diffs, self._prefix)

        return spfs.commit_layer(runtime)

    def _build_artifacts(self, env: MutableMapping[str, str] = None,) -> None:

        assert self._spec is not None

        pkg = self._spec.pkg

        os.makedirs(self._prefix, exist_ok=True)

        metadata_dir = data_path(pkg, prefix=self._prefix)
        build_spec = build_spec_path(pkg, prefix=self._prefix)
        build_options = build_options_path(pkg, prefix=self._prefix)
        build_script = build_script_path(pkg, prefix=self._prefix)
        os.makedirs(metadata_dir, exist_ok=True)
        with open(build_spec, "w+b") as bwriter:
            bwriter.write(api.write_spec(self._spec))
        with open(build_script, "w+") as writer:
            writer.write(self._spec.build.script)
        with open(build_options, "w+") as writer:
            json.dump(self._all_options, writer, indent="\t")

        env = env or {}
        env.update(self._all_options.to_environment())
        env["PREFIX"] = self._prefix

        if isinstance(self._source, api.Ident):
            source_dir = data_path(self._source, prefix=self._prefix)
        else:
            source_dir = os.path.abspath(self._source)

        cmd = spfs.build_shell_initialized_command("/bin/sh", "-ex", build_script)
        proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
        proc.wait()
        if proc.returncode != 0:
            raise BuildError(
                f"Build script returned non-zero exit status: {proc.returncode}"
            )


def build_spec_path(pkg: api.Ident, prefix: str = "/spfs") -> str:
    """Return the file path for the given build's spec.yaml file.

    This file is created during a build and stores the full
    package spec of what was built.
    """
    return os.path.join(data_path(pkg, prefix), "spec.yaml")


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
            if diff.mode is spfs.tracking.DiffMode.changed and diff.entries:
                mode_change = diff.entries[0].mode ^ diff.entries[1].mode
                nonperm_change = (mode_change | 0o777) ^ 0o777
                if mode_change and not nonperm_change:
                    # NOTE(rbottriell):permission changes are not properly reset by spfs
                    # so we must deal with them manually for now
                    os.chmod(prefix + diff.path, diff.entries[0].mode)
                    continue
            raise BuildError(f"Existing file was {diff.mode.name}: {prefix}{diff.path}")
