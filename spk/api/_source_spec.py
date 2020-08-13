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
    def subdir(self) -> Optional[str]:

        return None

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


@dataclass
class LocalSource(SourceSpec):
    """Package source files in a local file path."""

    path: str = "."

    def collect(self, dirname: str) -> None:

        dirname = os.path.join(dirname, "")  # require trailing '/' for rsync semantics
        path = os.path.join(self.path, "")  # require trailing '/' for rsync semantics
        args = ["--recursive", "--archive"]
        if "SPM_DEBUG" in os.environ:
            args.append("--verbose")
        if os.path.exists(os.path.join(path, ".gitignore")):
            args += ["--filter", ":- .gitignore"]
        args += ["--cvs-exclude", path, dirname]
        cmd = ["rsync"] + args
        _LOGGER.debug(" ".join(cmd))
        subprocess.check_call(cmd, cwd=dirname)

    def to_dict(self) -> Dict[str, Any]:
        return {"path": self.path}

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "LocalSource":

        return LocalSource(data["path"])


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

        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "GitSource":

        return GitSource(
            git=data["git"],
            ref=str(data.get("ref", "")),
            depth=int(data.get("depth", 1)),
        )


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
        out = {"tar": self.tar}
        return out

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TarSource":

        return TarSource(data["tar"])
