from typing import Dict, List, Any
from dataclasses import dataclass, field

from ._request import Request
from ._option_map import OptionMap


@dataclass
class TestSpec:
    """A set of structured inputs used to build a package."""

    stage: str
    script: str
    selectors: List[OptionMap] = field(default_factory=list)
    requirements: List[Request] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {
            "stage": self.stage,
            "script": self.script.splitlines(),
        }
        if self.selectors:
            spec["selectors"] = [dict(s) for s in self.selectors]
        if self.requirements:
            spec["requirements"] = [r.to_dict() for r in self.requirements]
        return spec

    @staticmethod
    def from_dict(data: Dict[str, Any]) -> "TestSpec":
        """Construct a TestSpec from a dictionary config."""

        stage = data.pop("stage")
        script = data.pop("script")
        if isinstance(script, list):
            script = "\n".join(script)

        ts = TestSpec(stage, script)

        selectors = data.pop("selectors", [])
        if not isinstance(selectors, list):
            raise ValueError(f"test.selectors must be a list, got {type(selectors)}")
        ts.selectors = [OptionMap(**data) for data in selectors]

        requirements = data.pop("requirements", [])
        if not isinstance(requirements, list):
            raise ValueError(
                f"test.requirements must be a list, got {type(requirements)}"
            )
        ts.requirements = [Request.from_dict(r) for r in requirements]

        if len(data):
            raise ValueError(
                f"unrecognized fields in spec.tests: {', '.join(data.keys())}"
            )
        return ts
