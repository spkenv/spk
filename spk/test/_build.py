# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Iterable, List, Union
import os
import subprocess
import tempfile

import spkrs

from .. import api, solve, exec, build, storage


class TestError(RuntimeError):
    """Denotes an error during the testing process."""

    pass


class PackageBuildTester:
    def __init__(self, spec: api.Spec, script: str) -> None:
        self._prefix = "/spfs"
        self._spec = spec
        self._script = script
        self._repos: List[storage.Repository] = []
        self._options = api.OptionMap()
        self._additional_requirements: List[api.Request] = []
        self._source: Union[str, api.Ident] = spec.pkg.with_build(api.SRC)
        self._solver = solve.Solver()

    def get_solve_graph(self) -> solve.Graph:
        """Return the solver graph for the test environment.

        This is most useful for debugging test environments that failed to resolve,
        and test that failed with a SolverError.

        If the tester has not run, returns an incomplete.
        """

        return self._solver.get_last_solve_graph()

    def with_option(self, name: str, value: str) -> "PackageBuildTester":

        self._options[name] = value
        return self

    def with_options(self, options: api.OptionMap) -> "PackageBuildTester":

        self._options.update(options)
        return self

    def with_repository(self, repo: storage.Repository) -> "PackageBuildTester":

        self._repos.append(repo)
        return self

    def with_source(self, source: Union[str, api.Ident]) -> "PackageBuildTester":

        self._source = source
        return self

    def with_repositories(
        self, repos: Iterable[storage.Repository]
    ) -> "PackageBuildTester":

        self._repos.extend(repos)
        return self

    def with_requirements(
        self, requests: Iterable[api.Request]
    ) -> "PackageBuildTester":

        self._additional_requirements.extend(requests)
        return self

    def test(self) -> None:

        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])

        solution = self._resolve_source_package()
        stack = exec.resolve_runtime_layers(solution)
        spkrs.reconfigure_runtime(stack=stack)

        self._solver.reset()
        for request in self._additional_requirements:
            self._solver.add_request(request)
        self._solver.update_options(self._options)
        for repo in self._repos:
            self._solver.add_repository(repo)
        if isinstance(self._source, api.Ident):
            ident_range = api.parse_ident_range(
                f"{self._source.name}/={self._source.version}/{self._source.build}"
            )
            request = api.PkgRequest(ident_range, api.PreReleasePolicy.IncludeAll)
            self._solver.add_request(request)
        solution = self._solver.solve_build_environment(self._spec)

        stack = exec.resolve_runtime_layers(solution)
        spkrs.reconfigure_runtime(stack=stack)

        specs = list(s for _, s, _ in solution.items())
        self._options.update(solution.options())
        self._spec.update_for_build(self._options, specs)

        env = solution.to_environment(os.environ)
        env = self._spec.resolve_all_options(solution.options()).to_environment(env)
        env.update(build.get_package_build_env(self._spec))
        env["PREFIX"] = self._prefix

        source_dir = build.source_package_path(
            self._spec.pkg.with_build(api.SRC), self._prefix
        )
        with tempfile.NamedTemporaryFile("w+") as script_file:
            script_file.write(self._script)
            script_file.flush()
            os.environ["SHELL"] = "sh"
            cmd = spkrs.build_shell_initialized_command(
                "/bin/sh", "-ex", script_file.name
            )

            with build.deferred_signals():
                proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
                proc.wait()
            if proc.returncode != 0:
                raise TestError(
                    f"Test script returned non-zero exit status: {proc.returncode}"
                )

    def _resolve_source_package(self) -> solve.Solution:

        self._solver.reset()
        self._solver.update_options(self._options)
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
            request = api.PkgRequest(ident_range, api.PreReleasePolicy.IncludeAll)
            self._solver.add_request(request)

        return self._solver.solve()
