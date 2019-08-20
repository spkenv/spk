from typing import List, Union
import os
import re
import uuid
import errno
import shutil
import tarfile
import hashlib
import subprocess

import git


class NoRepositoryError(ValueError):
    pass


class UnknownVersionError(ValueError):

    pattern = re.compile(
        r"^fatal: invalid reference: (.*)$", flags=re.RegexFlag.MULTILINE
    )


class Repository:
    def __init__(self, root: str):

        self._root = os.path.abspath(root)
        try:
            self._repo = git.Repo(self._root)
        except git.NoSuchPathError:
            raise NoRepositoryError(f"Not a repository: {root}")

    def add_worktree(self, location, commitish) -> git.Repo:

        location = os.path.abspath(location)
        proc = subprocess.Popen(
            ["git", "worktree", "add", location, commitish],
            cwd=self._root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        _, err = proc.communicate()
        if proc.returncode == 0:
            return git.Repo(location)

        stderr = err.decode("utf-8")

        no_ref_err = UnknownVersionError.pattern.match(stderr)
        if no_ref_err:
            raise UnknownVersionError(no_ref_err.group(1))

        raise RuntimeError(stderr)

    # def read_ref(self, ref: str) -> Ref:

    #     # TODO: refs directory

    #     try:
    #         return self.layers.read_layer(ref)
    #     except ValueError:
    #         pass

    #     try:
    #         return self.runtimes.read_runtime(ref)
    #     except ValueError:
    #         pass

    #     raise ValueError(f"Unknown ref: {ref}")

    # def commit(self, ref: str) -> Layer:

    #     runtime = self.read_ref(ref)
    #     if not isinstance(runtime, Runtime):
    #         raise ValueError(f"Not a runtime: {ref}")

    #     return self.layers.commit_runtime(runtime)


class RepositoryStorage:
    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    def create_repository(self, tag_path: str) -> Repository:

        repo_path = os.path.join(self._root, tag_path + ".git")
        os.makedirs(repo_path, exist_ok=True)
        try:
            git.Repo.init(repo_path, bare=True)
        except git.InvalidGitRepositoryError:
            raise ValueError(f"Repository exists: {tag_path}")
        return Repository(repo_path)

    def read_repository(self, tag_path: str) -> Repository:

        repo_path = os.path.join(self._root, tag_path + ".git")
        return Repository(repo_path)
