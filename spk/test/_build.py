from typing import Iterable, List, Optional, Union
import os
import signal
import subprocess
import tempfile

import spfs

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
        self._source: Union[str, api.Ident] = spec.pkg.with_build(api.SRC)
        self._solver: Optional[solve.Solver] = None

    def get_test_env_decision_tree(self) -> solve.DecisionTree:
        """Return the solver decision tree for the test environment.

        This is most useful for debugging test environments that failed to resolve,
        and test that failed with a SolverError.

        If the tester has not run, return an empty tree.
        """

        if self._solver is None:
            return solve.DecisionTree()
        return self._solver.decision_tree

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

    def test(self) -> None:

        runtime = spfs.active_runtime()
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)
        runtime.reset("**/*")
        runtime.reset_stack()
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)

        solution = self._resolve_source_package()
        exec.configure_runtime(runtime, solution)

        self._solver = solve.Solver(self._options)
        for repo in self._repos:
            self._solver.add_repository(repo)
        solution = self._solver.solve_build_environment(self._spec)

        exec.configure_runtime(runtime, solution)
        runtime.set_editable(True)
        spfs.remount_runtime(runtime)

        specs = list(s for _, s, _ in solution.items())
        self._options.update(solution.options())
        self._spec.update_for_build(self._options, specs)

        env = solution.to_environment() or {}
        env.update(self._spec.resolve_all_options(solution.options()).to_environment())
        env.update(build.get_package_build_env(self._spec))
        env["PREFIX"] = self._prefix

        source_dir = build.source_package_path(
            self._spec.pkg.with_build(api.SRC), self._prefix
        )
        with tempfile.NamedTemporaryFile("w+") as script_file:
            script_file.write(self._script)
            script_file.flush()
            os.environ["SHELL"] = "sh"
            cmd = spfs.build_shell_initialized_command(
                "/bin/sh", "-ex", script_file.name
            )

            # do not react to os signals while the subprocess is running,
            # these should be handled by the underlying process instead
            signal.signal(signal.SIGINT, lambda *_: None)
            signal.signal(signal.SIGTERM, lambda *_: None)
            try:
                proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
                proc.wait()
                if proc.returncode != 0:
                    raise TestError(
                        f"Test script returned non-zero exit status: {proc.returncode}"
                    )
            finally:
                signal.signal(signal.SIGINT, signal.SIG_DFL)
                signal.signal(signal.SIGTERM, signal.SIG_DFL)

    def _resolve_source_package(self) -> solve.Solution:

        self._solver = solve.Solver(self._options)
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
