from typing import Dict, List, Any, Optional, Tuple, Union, Set
import os
import abc
import enum
from dataclasses import dataclass, field

from ._option_map import OptionMap


@dataclass
class TestSpec:
    """A set of structured inputs used to build a package."""

    stage: str
    script: str
    selectors: List[OptionMap] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {
            "stage": self.stage,
            "script": self.script.splitlines(),
        }
        if self.selectors:
            spec["selectors"] = [s.to_dict() for s in self.selectors]
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TestSpec":
        """Construct a TestSpec from a dictionary config."""

        stage = data.pop("stage")
        script = data.pop("script")
        if isinstance(script, list):
            script = "\n".join(script)

        ts = TestSpec(stage, script)
        ts.selectors = [OptionMap(**data) for data in data.pop("selectors", [])]

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.tests: {', '.join(data.keys())}"
            )
        return ts
