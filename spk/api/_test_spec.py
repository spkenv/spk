from typing import Dict, List, Any, Optional, Tuple, Union, Set
import os
import abc
import enum
from dataclasses import dataclass, field

from ._request import Request, PkgRequest, parse_ident_range, PreReleasePolicy
from ._option_map import OptionMap
from ._name import validate_name
from ._compat import Compatibility, COMPATIBLE


@dataclass
class TestSpec:
    """A set of structured inputs used to build a package."""

    stage: str
    script: str

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {
            "stage": self.stage,
            "script": self.script.splitlines(),
        }
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TestSpec":
        """Construct a TestSpec from a dictionary config."""

        stage = data.pop("stage")
        script = data.pop("script")
        if isinstance(script, list):
            script = "\n".join(script)

        ts = TestSpec(stage, script)

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.tests: {', '.join(data.keys())}"
            )
        return ts
