from typing import List, Union
import os
import re
import uuid
import errno
import shutil
import tarfile
import hashlib
import subprocess
import urllib.parse

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


class RepositoryStorage:
    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    def create_local_repository(self, tag_path: str) -> Repository:

        repo_path = os.path.join(self._root, tag_path + ".git")
        try:
            os.makedirs(repo_path)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Repository exists: " + tag_path)
            raise
        repo = git.Repo.init(repo_path, bare=True)
        repo.index.commit("Initialize empty master branch")
        return self.read_repository(tag_path)

    def clone_repository(self, tag_path: str) -> Repository:

        repo_path = os.path.join(self._root, tag_path + ".git")
        try:
            os.makedirs(repo_path)
        except OSError as e:
            if e.errno == errno.EEXIST:
                raise ValueError("Repository exists: " + tag_path)
            raise

        schemes = (
            lambda p: f"git@{p}.git".replace("/", ":", 1),
            lambda p: f"https://{p}",
            lambda p: f"http://{p}",
            lambda p: f"file:{p}",
        )

        for scheme in schemes:
            git_url = scheme(tag_path)
            proc = subprocess.Popen(
                ["git", "ls-remote", "--exit-code", "-h", git_url],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            try:
                out, err = proc.communicate(timeout=1)
            except subprocess.TimeoutExpired:
                proc.kill()
                continue
            if proc.returncode != 0:
                continue
            try:
                git.Repo.clone_from(git_url, repo_path, bare=True)
            except git.GitCommandError as e:
                continue
        else:
            os.rmdir(repo_path)
            raise RuntimeError("Failed to find remote source for: " + tag_path)

        raise NotImplementedError("TODO: manage clone")

    def read_repository(self, tag_path: str) -> Repository:

        repo_path = os.path.join(self._root, tag_path + ".git")
        return Repository(repo_path)
