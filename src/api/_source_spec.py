# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Dict, List, Any, Optional
import abc
import os
import re
import tempfile
import subprocess
import functools
from dataclasses import dataclass, field

import structlog

_LOGGER = structlog.get_logger("spk.api")


class SourceSpec(metaclass=abc.ABCMeta):
    @abc.abstractproperty
    def subdir(self) -> Optional[str]:
        """Optional directory under the main source folder to place these sources."""
        pass

    @abc.abstractmethod
    def collect(self, dirname: str) -> None:
        """Collect the represented sources files into the given directory."""

        pass

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "SourceSpec":

        if "path" in data:
            return LocalSource.from_dict(data)
        elif "git" in data:
            return GitSource.from_dict(data)
        elif "tar" in data:
            return TarSource.from_dict(data)
        elif "script" in data:
            return ScriptSource.from_dict(data)
        else:
            raise ValueError("Cannot determine type of source specifier")

    @abc.abstractmethod
    def to_dict(self) -> Dict[str, Any]:
        ...


@dataclass
class LocalSource(SourceSpec):
    """Package source files in a local directory or file path."""

    path: str = "."
    exclude: List[str] = field(default_factory=lambda: [".git/", ".svn/"])
    filter: List[str] = field(default_factory=lambda: [":- .gitignore"])
    subdir: Optional[str] = None

    def collect(self, dirname: str) -> None:

        args = ["--archive"]
        if os.path.isdir(self.path):
            # if the source path is a directory then we require
            # a trailing '/' so that rsync doesn't create new subdirectories
            # in the destination folder
            path = os.path.join(self.path, "")
            args.append("--recursive")
        else:
            path = self.path.rstrip(os.pathsep)
        # require a trailing '/' on destination also so that rsync doesn't
        # add aditional levels to the resulting structure
        dirname = os.path.join(dirname, "")
        if "SPK_DEBUG" in os.environ:
            args.append("--verbose")
        for filter_rule in self.filter:
            args += ["--filter", filter_rule]
        for exclusion in self.exclude:
            args += ["--exclude", exclusion]
        args += [path, dirname]
        cmd = ["rsync"] + args
        _LOGGER.debug(" ".join(cmd))
        subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"path": self.path}
        if self.subdir is not None:
            out["subdir"] = self.subdir
        if self.exclude != LocalSource().exclude:
            out["exclude"] = list(self.exclude)
        if self.filter != LocalSource().filter:
            out["filter"] = list(self.filter)
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "LocalSource":

        src = LocalSource(data.pop("path"))
        src.subdir = data.pop("subdir", None)

        if "exclude" in data:
            src.exclude = data.pop("exclude")
            assert isinstance(
                src.exclude, list
            ), "LocalSource.exclude must be a list of strings"

        if "filter" in data:
            src.filter = data.pop("filter")
            assert isinstance(
                src.filter, list
            ), "LocalSource.filter must be a list of strings"

        for name in data:
            raise ValueError(f"Unknown field in LocalSource: '{name}'")

        return src


@dataclass
class GitSource(SourceSpec):
    """Package source files from a remote git repository."""

    git: str
    ref: str = ""
    depth: int = 1
    subdir: Optional[str] = None

    def collect(self, dirname: str) -> None:

        git_cmd = ["git", "clone", f"--depth={self.depth}"]
        if self.ref:
            git_cmd += ["-b", self.ref]
        git_cmd += [self.git, dirname]

        submodule_cmd = [
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
        ]
        if git_supports_submodule_depth():
            submodule_cmd += [f"--depth={self.depth}"]

        commands = [git_cmd, submodule_cmd]
        for cmd in commands:
            _LOGGER.debug(" ".join(cmd))
            subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"git": self.git}

        if self.ref:
            out["ref"] = self.ref
        if self.depth != 1:
            out["depth"] = self.depth
        if self.subdir is not None:
            out["subdir"] = self.subdir

        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "GitSource":

        src = GitSource(
            git=data.pop("git"),
            ref=str(data.pop("ref", "")),
            depth=int(data.pop("depth", 1)),
        )
        src.subdir = data.pop("subdir", None)

        for name in data:
            raise ValueError(f"Unknown field in GitSource: '{name}'")

        return src


@dataclass
class TarSource(SourceSpec):
    """Package source files from a local or remote tar archive."""

    tar: str
    subdir: Optional[str] = None

    def collect(self, dirname: str) -> None:

        with tempfile.TemporaryDirectory() as tmpdir:
            tarfile = os.path.join(tmpdir, os.path.basename(self.tar))
            if re.match(r"^https?://", self.tar):
                cmd = ["wget", self.tar]
                _LOGGER.debug(" ".join(cmd))
                subprocess.check_call(cmd, cwd=tmpdir)
            else:
                tarfile = os.path.abspath(self.tar)

            cmd = ["tar", "-xf", tarfile]
            _LOGGER.debug(" ".join(cmd))
            subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"tar": self.tar}
        if self.subdir is not None:
            out["subdir"] = self.subdir
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TarSource":

        src = TarSource(data.pop("tar"))
        src.subdir = data.pop("subdir", None)

        for name in data:
            raise ValueError(f"Unknown field in TarSource: '{name}'")

        return src


@dataclass
class ScriptSource(SourceSpec):
    """Package source files collected via arbitrary shell script."""

    script: List[str] = field(default_factory=list)
    subdir: Optional[str] = None

    def collect(self, dirname: str) -> None:

        script_file = tempfile.NamedTemporaryFile("w")
        script_file.write("\n".join(self.script))
        script_file.flush()

        _LOGGER.debug(
            "running sources script",
            cmd=" ".join(["bash", "-ex", script_file.name]),
            cwd=dirname,
        )
        proc = subprocess.Popen(["bash", "-ex", script_file.name], cwd=dirname)
        proc.wait()
        if proc.returncode != 0:
            raise RuntimeError(
                f"sources script exited with non-zero status: {proc.returncode}"
            )

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"script": list(self.script)}
        if self.subdir is not None:
            out["subdir"] = self.subdir
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "ScriptSource":

        script = data.pop("script")
        if isinstance(script, str):
            script = [script]
        assert isinstance(
            script, list
        ), "sources.script must be a string or list of strings"

        src = ScriptSource(script)
        src.subdir = data.pop("subdir", None)

        for name in data:
            raise ValueError(f"Unknown field in ScriptSource: '{name}'")

        return src


def git_supports_submodule_depth() -> bool:

    v = git_version()
    return bool(v and v >= "2.0")


@functools.lru_cache()
def git_version() -> Optional[str]:

    try:
        out = subprocess.check_output(["git", "--version"])
    except subprocess.CalledProcessError:
        return None

    # eg: git version 1.83.6
    return out.decode().strip().split(" ")[-1]
