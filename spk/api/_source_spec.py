from typing import Dict, List, Any, Optional
import abc
import os
import re
import tempfile
import subprocess
from dataclasses import dataclass, field

from ._option_map import OptionMap
from ._ident import Ident

import structlog

_LOGGER = structlog.get_logger("spk.api")


class SourceSpec(metaclass=abc.ABCMeta):
    def __init__(self) -> None:
        self._subdir: Optional[str] = None

    def subdir(self) -> Optional[str]:

        return self._subdir

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
        if os.path.exists(os.path.join(path, ".gitignore")):
            args += ["--filter", ":- .gitignore"]
        for exclusion in self.exclude:
            args += ["--exclude", exclusion]
        args += [path, dirname]
        cmd = ["rsync"] + args
        _LOGGER.debug(" ".join(cmd))
        subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"path": self.path}
        if self.subdir() is not None:
            out["subdir"] = self.subdir()
        if self.exclude != LocalSource().exclude:
            out["exclude"] = list(self.exclude)
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "LocalSource":

        src = LocalSource(data.pop("path"))
        src._subdir = data.pop("subdir", None)

        if "exclude" in data:
            src.exclude = data.pop("exclude")
            assert isinstance(
                src.exclude, list
            ), "LocalSource.exclude must be a list of strings"

        for name in data:
            raise ValueError(f"Unknown field in LocalSource: '{name}'")

        return src


@dataclass
class GitSource(SourceSpec):
    """Package source files from a remote git repository."""

    git: str
    ref: str = ""
    depth: int = 1

    def collect(self, dirname: str) -> None:

        git_cmd = ["git", "clone", f"--depth={self.depth}"]
        if self.ref:
            git_cmd += ["-b", self.ref]
        git_cmd += [self.git, dirname]

        commands = [
            git_cmd,
            [
                "git",
                "submodule",
                "update",
                "--init",
                "--recursive",
                f"--depth={self.depth}",
            ],
        ]
        for cmd in commands:
            _LOGGER.debug(" ".join(cmd))
            subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"git": self.git}

        if self.ref:
            out["ref"] = self.ref
        if self.depth != 1:
            out["depth"] = self.depth
        if self.subdir() is not None:
            out["subdir"] = self.subdir()

        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "GitSource":

        src = GitSource(
            git=data.pop("git"),
            ref=str(data.pop("ref", "")),
            depth=int(data.pop("depth", 1)),
        )
        src._subdir = data.pop("subdir", None)

        for name in data:
            raise ValueError(f"Unknown field in GitSource: '{name}'")

        return src


@dataclass
class TarSource(SourceSpec):
    """Package source files from a local or remote tar archive."""

    tar: str

    def collect(self, dirname: str) -> None:

        with tempfile.TemporaryDirectory() as tmpdir:
            tarfile = os.path.join(tmpdir, os.path.dirname(self.tar))
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
        if self.subdir() is not None:
            out["subdir"] = self.subdir()
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TarSource":

        src = TarSource(data.pop("tar"))
        src._subdir = data.pop("subdir", None)

        for name in data:
            raise ValueError(f"Unknown field in TarSource: '{name}'")

        return src
