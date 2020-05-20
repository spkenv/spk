from typing import Dict, List, Any, Optional
import abc
import os
from dataclasses import dataclass, field

from ._option_map import OptionMap
from ._ident import Ident


class SourceSpec(metaclass=abc.ABCMeta):
    def subdir(self) -> Optional[str]:

        return None

    @abc.abstractmethod
    def script(self, dirname: str) -> str:

        pass

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "SourceSpec":

        if "path" in data:
            return LocalSource.from_dict(data)
        else:
            raise ValueError("Cannot determine type of source specifier")


@dataclass
class LocalSource(SourceSpec):
    """Package source files in a local file path."""

    path: str = "."

    def script(self, dirname: str) -> str:

        # TODO: work if .gitignore doesn't exist or not git repo
        args = ["--recursive", "--archive"]
        if "SPM_DEBUG" in os.environ:
            args.append("--verbose")
        args += ["--filter=':- .gitignore'", "--cvs-exclude", self.path, dirname]
        cmd = ["rsync"] + args
        return " ".join(cmd)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "path": self.path,
        }

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "LocalSource":

        return LocalSource(data["path"])
