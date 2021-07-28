from typing import List, Iterable, Optional, MutableMapping, Union, Dict
import os
import json
import subprocess

import structlog
import spkrs

from .. import api, storage, solve, exec
from ._env import data_path, deferred_signals

from spkrs.build import (
    build_options_path,
    build_script_path,
    build_spec_path,
    source_package_path,
)

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
    ... ).pkg
    my-pkg/0.0.0/3I42H3S6
    """

    def __init__(self) -> None:

        self._prefix = "/spfs"
        self._spec: Optional[api.Spec] = None
        self._all_options = api.OptionMap()
        self._pkg_options = api.OptionMap()
        self._source: Union[str, api.Ident] = "."
        self._solver = solve.Solver()
        self._repos: List[storage.Repository] = []
        self._interactive = False

    @staticmethod
    def from_spec(spec: api.Spec) -> "BinaryPackageBuilder":

        builder = BinaryPackageBuilder()
        builder._spec = spec.copy()
        builder._source = spec.pkg.with_build(api.SRC)
        return builder

    def get_solve_graph(self) -> solve.Graph:
        """Return the resolve graph from the build environment.

        This is most useful for debugging build environments that failed to resolve,
        and builds that failed with a SolverError.

        If the builder has not run, return an incomplete graph.
        """

        return self._solver.get_last_solve_graph()

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

    def set_interactive(self, interactive: bool) -> "BinaryPackageBuilder":

        self._interactive = interactive
        return self

    def build(self) -> api.Spec:
        """Build the requested binary package."""

        assert (
            self._spec is not None
        ), "Target spec not given, did you use BinaryPackagebuilder.from_spec?"

        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])
        _runtime = spkrs.active_runtime()

        self._pkg_options = self._spec.resolve_all_options(self._all_options)
        _LOGGER.debug("package options", options=self._pkg_options)
        compat = self._spec.build.validate_options(
            self._spec.pkg.name, self._all_options
        )
        if not compat:
            raise ValueError(compat)
        self._all_options.update(self._pkg_options)

        stack = []
        if isinstance(self._source, api.Ident):
            solution = self._resolve_source_package()
            stack = exec.resolve_runtime_layers(solution)
        solution = self._resolve_build_environment()
        opts = solution.options()
        opts.update(self._all_options)
        self._all_options = opts
        stack.extend(exec.resolve_runtime_layers(solution))
        spkrs.reconfigure_runtime(editable=True, stack=stack)

        specs = list(s for _, s, _ in solution.items())
        self._spec.update_spec_for_build(self._all_options, specs)
        env = os.environ.copy()
        env = solution.to_environment(env)
        env.update(self._all_options.to_environment())
        layer = self._build_and_commit_artifacts(env)
        storage.local_repository().publish_package(self._spec, layer)
        return self._spec

    def _resolve_source_package(self) -> solve.Solution:

        self._solver.reset()
        self._solver.update_options(self._all_options)
        self._solver.add_repository(storage.local_repository())
        for repo in self._repos:
            if repo == storage.local_repository():
                # local repo is always injected first, and duplicates are redundant
                continue
            self._solver.add_repository(repo)

        if isinstance(self._source, api.Ident):
            ident_range = api.parse_ident_range(
                f"{self._source.name}/={self._source.version}/{self._source.build}"
            )
            request = api.PkgRequest(ident_range, "IncludeAll")
            self._solver.add_request(request)

        return self._solver.solve()

    def _resolve_build_environment(self) -> solve.Solution:

        self._solver.reset()
        self._solver.update_options(self._all_options)
        self._solver.set_binary_only(True)
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
    ) -> spkrs.Digest:

        assert self._spec is not None, "Internal Error: spec is None"

        self._build_artifacts(env)

        sources_dir = data_path(self._spec.pkg.with_build(api.SRC), prefix=self._prefix)

        runtime = spkrs.active_runtime()
        pattern = os.path.join(sources_dir[len(self._prefix) :], "**")
        _LOGGER.info("Purging all changes made to source directory", dir=sources_dir)
        spkrs.reconfigure_runtime(reset=[pattern])

        _LOGGER.info("Validating package fileset...")
        try:
            spkrs.build.validate_build_changeset()
        except RuntimeError as e:
            raise BuildError(str(e))

        return spkrs.commit_layer(runtime)

    def _build_artifacts(
        self,
        env: MutableMapping[str, str],
    ) -> None:

        assert self._spec is not None

        pkg = self._spec.pkg

        os.makedirs(self._prefix, exist_ok=True)

        metadata_dir = data_path(pkg, prefix=self._prefix)
        build_spec = build_spec_path(pkg, prefix=self._prefix)
        build_options = build_options_path(pkg, prefix=self._prefix)
        build_script = build_script_path(pkg, prefix=self._prefix)
        os.makedirs(metadata_dir, exist_ok=True)
        api.save_spec_file(build_spec, self._spec)
        with open(build_script, "w+") as writer:
            writer.write("\n".join(self._spec.build.script))
        with open(build_options, "w+") as writer:
            json.dump(dict(self._all_options.items()), writer, indent="\t")

        env.update(self._all_options.to_environment())
        env.update(get_package_build_env(self._spec))
        env["PREFIX"] = self._prefix

        if isinstance(self._source, api.Ident):
            source_dir = source_package_path(self._source, self._prefix)
        else:
            source_dir = os.path.abspath(self._source)

        # force the base environment to be setup using bash, so that the
        # spfs startup and build environment are predictable and consistent
        # (eg in case the user's shell does not have startup scripts in
        #  the dependencies, is not supported by spfs, etc)
        if self._interactive:
            os.environ["SHELL"] = "bash"
            print("\nNow entering an interactive build shell")
            print(" - your current directory will be set to the sources area")
            print(" - build and install your artifacts into /spfs")
            print(" - this package's build script can be run from: " + build_script)
            print(" - to cancel and discard this build, run `exit 1`")
            print(" - to finalize and save the package, run `exit 0`")
            cmd = spkrs.build_interactive_shell_command()
        else:
            os.environ["SHELL"] = "bash"
            cmd = spkrs.build_shell_initialized_command("bash", "-ex", build_script)
        with deferred_signals():
            proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
            proc.wait()
        if proc.returncode != 0:
            raise BuildError(
                f"Build script returned non-zero exit status: {proc.returncode}"
            )


def get_package_build_env(spec: api.Spec) -> Dict[str, str]:
    """Return the environment variables to be set for a build of the given package spec."""

    return {
        "SPK_PKG": str(spec.pkg),
        "SPK_PKG_NAME": str(spec.pkg.name),
        "SPK_PKG_VERSION": str(spec.pkg.version),
        "SPK_PKG_BUILD": str(spec.pkg.build or ""),
        "SPK_PKG_VERSION_MAJOR": str(spec.pkg.version.major),
        "SPK_PKG_VERSION_MINOR": str(spec.pkg.version.minor),
        "SPK_PKG_VERSION_PATCH": str(spec.pkg.version.patch),
        "SPK_PKG_VERSION_BASE": str(
            api.VERSION_SEP.join(str(p) for p in spec.pkg.version.parts)
        ),
    }
